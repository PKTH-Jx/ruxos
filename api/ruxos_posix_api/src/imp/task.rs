/* Copyright (c) [2023] [Syswonder Community]
 *   [Ruxos] is licensed under Mulan PSL v2.
 *   You can use this software according to the terms and conditions of the Mulan PSL v2.
 *   You may obtain a copy of Mulan PSL v2 at:
 *               http://license.coscl.org.cn/MulanPSL2
 *   THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
 *   See the Mulan PSL v2 for more details.
 */

use crate::ctypes;
use core::ffi::c_int;

use ruxtask::{task::PROCESS_MAP, yield_now};

/// Relinquish the CPU, and switches to another task.
///
/// For single-threaded configuration (`multitask` feature is disabled), we just
/// relax the CPU and wait for incoming interrupts.
pub fn sys_sched_yield() -> c_int {
    #[cfg(feature = "multitask")]
    ruxtask::yield_now();
    #[cfg(not(feature = "multitask"))]
    if cfg!(feature = "irq") {
        ruxhal::arch::wait_for_irqs();
    } else {
        core::hint::spin_loop();
    }
    0
}

/// Get current thread ID.
pub fn sys_gettid() -> c_int {
    syscall_body!(sys_gettid,
        #[cfg(feature = "multitask")]
        {
            Ok(ruxtask::current().id().as_u64() as c_int)
        }
        #[cfg(not(feature = "multitask"))]
        {
            Ok(2) // `main` task ID
        }
    )
}

/// Get current process ID.
pub fn sys_getpid() -> c_int {
    syscall_body!(sys_getpid, Ok(ruxtask::current().id().as_u64() as c_int))
}

/// Get parent process's ID.
pub fn sys_getppid() -> c_int {
    syscall_body!(sys_getppid, Ok(1))
}

/// Wait for a child process to exit and return its status.
///
/// TOSO, wstatus, options, and rusage are not implemented yet.
pub fn sys_wait4(
    pid: c_int,
    wstatus: *mut c_int,
    options: c_int,
    rusage: *mut ctypes::rusage,
) -> c_int {
    const WNOHANG: c_int = 0x00000001;

    error!(
        "sys_wait4 <= pid: {}, wstatus: {:?}, options: {}, rusage: {:?}",
        pid, wstatus, options, rusage
    );

    if pid > 0 {
        loop {
            let mut process_map = PROCESS_MAP.lock();
            if let Some(task) = process_map.get(&(pid as u64)) {
                if task.state() == ruxtask::task::TaskState::Exited {
                    process_map.remove(&(pid as u64));
                    return pid;
                } else if options & WNOHANG != 0 {
                    return 0; // No child process
                }
            } else {
                return -1; // No such process
            }
            // drop lock before yielding to other tasks
            drop(process_map);
            // for single-cpu system, we must yield to other tasks instead of dead-looping here.
            yield_now();
        }
    } else if pid == -1 {
        let mut to_remove: Option<u64> = None;
        while to_remove.is_none() {
            let process_map = PROCESS_MAP.lock();
            for (child_pid, task) in process_map
                .iter()
                .filter(|(_, task)| task.parent_process().is_some())
            {
                let parent_pid = task.parent_process().unwrap().id().as_u64();
                if parent_pid == ruxtask::current().id().as_u64() {
                    if task.state() == ruxtask::task::TaskState::Exited {
                        // add to to_remove list
                        let _ = to_remove.insert(*child_pid);
                        break;
                    }
                }
            }
            if options & WNOHANG != 0 {
                return 0; // No child process
            }
            // drop lock before yielding to other tasks
            drop(process_map);
            // for single-cpu system, we must yield to other tasks instead of dead-looping here.
            yield_now();
        }
        PROCESS_MAP.lock().remove(&to_remove.unwrap());
        return to_remove.unwrap() as c_int;
    } else {
        return -1; // Invalid argument
    }
}

/// Exit current task
pub fn sys_exit(exit_code: c_int) -> ! {
    debug!("sys_exit <= {}", exit_code);
    #[cfg(feature = "multitask")]
    ruxtask::exit(exit_code);
    #[cfg(not(feature = "multitask"))]
    ruxhal::misc::terminate();
}
