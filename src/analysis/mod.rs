//! Analysis module for signal processing
//!
//! This module provides signal analysis tools including:
//! - FFT (Fast Fourier Transform) for frequency domain analysis
//! - Power spectral density computation
//! - Peak detection

pub mod fft;

pub use fft::{FftAnalyzer, FftConfig, FftResult, WindowFunction};
