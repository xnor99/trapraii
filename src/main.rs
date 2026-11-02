use std::{
    env,
    ffi::OsStr,
    os::unix::process::ExitStatusExt,
    process::{Command, ExitCode},
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
};

use signal_hook::{
    consts::{SIGHUP, SIGINT, SIGTERM},
    iterator::{Handle, Signals},
};

struct InterruptState {
    interrupted: Mutex<bool>,
    cond_var: Condvar,
}

impl InterruptState {
    fn new() -> Self {
        Self {
            interrupted: Mutex::new(false),
            cond_var: Condvar::new(),
        }
    }
}

struct CliArgs {
    magic_error_code: Option<u8>,
    command1: Vec<String>,
    command2: Vec<String>,
}

fn get_args() -> Option<CliArgs> {
    let args = env::args().skip(1).collect::<Vec<_>>();

    if args.len() < 2 {
        return None;
    }

    let options = args.get(..args.len() - 2)?;
    let mut magic_error_code = None;
    let command1_str = args.get(args.len() - 2)?;
    let command2_str = args.last()?;
    {
        let mut i = 0;
        while i < options.len() {
            if options.get(i)? == "--magic-error-code" {
                magic_error_code = Some(options.get(i + 1)?.parse().ok()?);
                i += 1;
            } else {
                return None;
            }
            i += 1;
        }
    }
    Some(CliArgs {
        magic_error_code,
        command1: shell_words::split(command1_str).ok()?,
        command2: shell_words::split(command2_str).ok()?,
    })
}

fn handle_signals(state: Arc<InterruptState>) -> Option<(JoinHandle<()>, Handle)> {
    let mut signals = Signals::new([SIGHUP, SIGINT, SIGTERM]).ok()?;
    let handle = signals.handle();
    let handle_thread = handle.clone();
    Some((
        thread::spawn(move || {
            signals.forever().for_each(move |_| {
                *state.interrupted.lock().unwrap() = true;
                state.cond_var.notify_one();
                handle_thread.close();
            });
        }),
        handle,
    ))
}

fn run_command<S: AsRef<OsStr>>(command: &[S]) -> Option<i32> {
    let program = command.first()?;
    let args = command.get(1..)?;
    Command::new(program)
        .args(args)
        .status()
        .ok()
        .and_then(|exit_status| {
            exit_status
                .code()
                .or_else(move || exit_status.signal().map(|signal| signal + 128))
        })
}

struct JoinOnDrop<T> {
    join_handle: Option<JoinHandle<T>>,
    signal_handle: Handle,
}

impl<T> JoinOnDrop<T> {
    fn new(join_handle: JoinHandle<T>, signal_handle: Handle) -> Self {
        Self {
            join_handle: Some(join_handle),
            signal_handle,
        }
    }
}

impl<T> Drop for JoinOnDrop<T> {
    fn drop(&mut self) {
        self.signal_handle.close();
        self.join_handle.take().unwrap().join().unwrap();
    }
}

fn main() -> ExitCode {
    fn main_inner(args: CliArgs) -> Option<ExitCode> {
        let state = Arc::new(InterruptState::new());
        let (thread_handle, handle) = handle_signals(state.clone())?;
        let _handle = JoinOnDrop::new(thread_handle, handle);
        let first_command_exit_code = run_command(&args.command1)?;
        if first_command_exit_code != 0 {
            return Some(ExitCode::from(first_command_exit_code as u8));
        }
        drop(
            state
                .cond_var
                .wait_while(state.interrupted.lock().unwrap(), |&mut interrupted| {
                    !interrupted
                })
                .unwrap(),
        );
        let second_command_exit_code = run_command(&args.command2)?;
        Some(ExitCode::from(second_command_exit_code as u8))
    }

    const MAGIC_ERROR_CODE: u8 = 42;
    let Some(args) = get_args() else {
        eprintln!("usage: trapraii [--magic-error-code val] command1 command2");
        return MAGIC_ERROR_CODE.into();
    };
    let error_code = args.magic_error_code;

    match main_inner(args) {
        Some(exit_code) => exit_code,
        None => error_code.unwrap_or(MAGIC_ERROR_CODE).into(),
    }
}
