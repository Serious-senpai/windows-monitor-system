use std::error::Error;
use std::io::{Write, stdin, stdout};
use std::sync::Arc;
use std::time::Duration;
use std::{env, process};

use clap::Parser;
use mock_events::cli::Arguments;
use tokio::sync::SetOnce;
use tokio::time::sleep;
use tokio::{fs, signal};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let arguments = Arguments::parse();
    let executable_path = env::current_exe().expect("Failed to get current executable path");
    let app_directory = executable_path
        .parent()
        .expect("Failed to get application directory")
        .to_path_buf();

    print!("Current PID is {}. Press Enter to start.", process::id());
    let _ = stdout().flush();

    let mut buf = String::new();
    let _ = stdin().read_line(&mut buf);

    let stopped = Arc::new(SetOnce::new());
    let stopped_clone = stopped.clone();
    tokio::spawn(async move {
        let _ = signal::ctrl_c().await;

        println!("Received Ctrl+C");
        let _ = stopped_clone.set(());
    });

    while stopped.get().is_none() {
        let mut tasks = vec![];
        for index in 0..arguments.files_count {
            let path = app_directory.join(format!("mock-{index}.tmp"));
            tasks.push(tokio::spawn(async move {
                let file = fs::File::create(&path)
                    .await
                    .unwrap_or_else(|_| panic!("Failed to create {}", path.display()));
                drop(file);
                fs::remove_file(&path)
                    .await
                    .unwrap_or_else(|_| panic!("Failed to remove {}", path.display()));
            }));
        }

        for task in tasks {
            if let Err(e) = task.await {
                println!("Task failed with error: {e}");
            }
        }

        println!("Finished 1 batch");
        sleep(Duration::from_millis(arguments.interval_ms)).await;
    }

    Ok(())
}
