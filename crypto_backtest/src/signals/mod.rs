// src/signals/mod.rs

// Re-export the file manager functionality
pub mod file_manager;

// Make SignalFileManager available from the signals module
pub use file_manager::SignalFileManager;

// You can add more signal-related modules here in the future
// For example:
// pub mod generator;
// pub mod processor;