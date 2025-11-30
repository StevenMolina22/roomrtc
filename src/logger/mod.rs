use std::{
    fs::OpenOptions,
    io::Write,
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use chrono::Local;

/// Represents the severity level of a log message.
#[derive(Debug)]
pub enum LogLevel {
    /// Detailed information, typically only useful when debugging.
    Debug,
    /// General operational information about the application flow.
    Info,
    /// Potentially harmful situations or unusual component behavior.
    Warn,
    /// Errors that prevent normal operation or flow.
    Error,
}

/// The structure containing all data required for a single log entry.
/// This struct is sent across the channel to the logging thread.
pub struct LogMessage {
    /// The severity level of the message.
    level: LogLevel,
    /// The body text of the log entry.
    message: String,
    /// The component or context generating the message (e.g., "IceAgent", "Controller").
    context: String,
    /// The timestamp when the message was generated.
    timestamp: String,
}

impl std::fmt::Display for LogLevel {
    /// Formats the LogLevel variant as its canonical uppercase string representation.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
            LogLevel::Debug => "DEBUG",
        };
        write!(f, "{}", s)
    }
}

/// A thread-safe, clonable handle used by application components to send log messages.
///
/// This structure acts as the Producer in the MPSC channel pattern, routing all
/// log calls to a single background thread responsible for file I/O.
#[derive(Clone)]
pub struct Logger {
    /// The Sender side of the channel, used to dispatch LogMessage instances.
    message_tx: Sender<LogMessage>,
    /// The specific context/name for this logger instance (e.g., "IceAgent").
    context: String,
}

impl Logger {
    /// Creates a new Logger instance and spawns the background logging thread.
    ///
    /// This method performs the following steps:
    /// 1. Opens the specified log file in append mode.
    /// 2. Creates the MPSC channel (`tx` and `rx`).
    /// 3. Spawns a dedicated thread that takes ownership of the file handle and the Receiver.
    ///    The thread uses `for msg in rx` for idiomatic, blocking message consumption.
    ///
    /// # Errors
    /// Returns an `std::io::Error` if the log file cannot be created or opened.
    pub fn new(file_path: &str) -> Result<Self, std::io::Error> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(file_path)?;

        let (tx, rx): (Sender<LogMessage>, Receiver<LogMessage>) = mpsc::channel();

        thread::spawn(move || {
            let mut writer = file;
            for msg in rx {
                let log_line = format!(
                    "[{}] [{}] [{}] {}\n",
                    msg.timestamp, msg.level, msg.context, msg.message
                );
                let _ = writer.write_all(log_line.as_bytes());
            }
        });

        Ok(Logger {
            message_tx: tx,
            context: String::new(),
        })
    }

    /// Sends a log message with the 'Info' severity level.
    ///
    /// The message is asynchronously processed by the logging thread.
    pub fn info(&self, message: &str) {
        self.send(LogLevel::Info, message);
    }

    /// Sends a log message with the 'Warn' severity level.
    ///
    /// The message is asynchronously processed by the logging thread.
    pub fn warn(&self, message: &str) {
        self.send(LogLevel::Warn, message);
    }

    /// Sends a log message with the 'Error' severity level.
    ///
    /// The message is asynchronously processed by the logging thread.
    pub fn error(&self, message: &str) {
        self.send(LogLevel::Error, message);
    }

    /// Sends a log message with the 'Debug' severity level.
    ///
    /// The message is asynchronously processed by the logging thread.
    pub fn debug(&self, message: &str) {
        self.send(LogLevel::Debug, message);
    }

    /// Internal method to package and send a message across the channel.
    ///
    /// It formats the current time, attaches the instance's context, and transmits
    /// the full `LogMessage` via the MPSC Sender.
    fn send(&self, level: LogLevel, message: &str) {
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let _ = self.message_tx.send(LogMessage {
            level,
            context: self.context.clone(),
            message: message.to_string(),
            timestamp,
        });
    }
}
