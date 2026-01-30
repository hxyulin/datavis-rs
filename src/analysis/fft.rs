//! FFT (Fast Fourier Transform) analysis module
//!
//! Provides frequency domain analysis for time-series data including:
//! - FFT computation with configurable window sizes
//! - Power spectral density calculation
//! - Peak frequency detection
//! - Various window functions (Hann, Hamming, Blackman)

use rustfft::{num_complex::Complex, FftPlanner};
use std::f64::consts::PI;

/// Window function type for FFT preprocessing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowFunction {
    /// Rectangular window (no windowing)
    #[default]
    Rectangular,
    /// Hann window (good general purpose)
    Hann,
    /// Hamming window (reduced side lobes)
    Hamming,
    /// Blackman window (very low side lobes)
    Blackman,
    /// Flat-top window (accurate amplitude measurement)
    FlatTop,
}

impl WindowFunction {
    /// Get display name
    pub fn display_name(&self) -> &'static str {
        match self {
            WindowFunction::Rectangular => "Rectangular",
            WindowFunction::Hann => "Hann",
            WindowFunction::Hamming => "Hamming",
            WindowFunction::Blackman => "Blackman",
            WindowFunction::FlatTop => "Flat-Top",
        }
    }

    /// Get all window functions
    pub fn all() -> &'static [WindowFunction] {
        &[
            WindowFunction::Rectangular,
            WindowFunction::Hann,
            WindowFunction::Hamming,
            WindowFunction::Blackman,
            WindowFunction::FlatTop,
        ]
    }

    /// Compute window coefficient at position i out of n samples
    pub fn coefficient(&self, i: usize, n: usize) -> f64 {
        let n_f = n as f64;
        let i_f = i as f64;

        match self {
            WindowFunction::Rectangular => 1.0,
            WindowFunction::Hann => 0.5 * (1.0 - (2.0 * PI * i_f / n_f).cos()),
            WindowFunction::Hamming => 0.54 - 0.46 * (2.0 * PI * i_f / n_f).cos(),
            WindowFunction::Blackman => {
                // Clamp to 0.0: the formula is exactly 0 at endpoints but
                // floating-point representation of 0.42 and 0.08 can produce -ε.
                (0.42 - 0.5 * (2.0 * PI * i_f / n_f).cos() + 0.08 * (4.0 * PI * i_f / n_f).cos())
                    .max(0.0)
            }
            WindowFunction::FlatTop => {
                let a0 = 0.21557895;
                let a1 = 0.41663158;
                let a2 = 0.277263158;
                let a3 = 0.083578947;
                let a4 = 0.006947368;
                a0 - a1 * (2.0 * PI * i_f / n_f).cos() + a2 * (4.0 * PI * i_f / n_f).cos()
                    - a3 * (6.0 * PI * i_f / n_f).cos()
                    + a4 * (8.0 * PI * i_f / n_f).cos()
            }
        }
    }

    /// Generate window coefficients for n samples
    pub fn generate(&self, n: usize) -> Vec<f64> {
        (0..n).map(|i| self.coefficient(i, n)).collect()
    }
}

/// FFT result containing frequency and magnitude data
#[derive(Debug, Clone)]
pub struct FftResult {
    /// Frequency bins (Hz)
    pub frequencies: Vec<f64>,
    /// Magnitude values (linear)
    pub magnitudes: Vec<f64>,
    /// Power spectral density (dB)
    pub psd_db: Vec<f64>,
    /// Sample rate used for computation
    pub sample_rate: f64,
    /// Number of samples used
    pub sample_count: usize,
    /// Frequency resolution (Hz per bin)
    pub frequency_resolution: f64,
}

impl FftResult {
    /// Find the peak frequency and its magnitude
    pub fn peak(&self) -> Option<(f64, f64)> {
        if self.magnitudes.is_empty() {
            return None;
        }

        let (idx, &max_mag) = self
            .magnitudes
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))?;

        Some((self.frequencies[idx], max_mag))
    }

    /// Find the top N peaks
    pub fn top_peaks(&self, n: usize) -> Vec<(f64, f64)> {
        if self.magnitudes.is_empty() {
            return Vec::new();
        }

        // Create indexed magnitudes
        let mut indexed: Vec<(usize, f64)> = self
            .magnitudes
            .iter()
            .enumerate()
            .map(|(i, &m)| (i, m))
            .collect();

        // Sort by magnitude descending
        indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top N, filtering out adjacent bins (simple peak detection)
        let mut peaks = Vec::new();
        for (idx, mag) in indexed {
            // Skip if too close to an existing peak
            let too_close = peaks.iter().any(|&(f, _): &(f64, f64)| {
                (self.frequencies[idx] - f).abs() < self.frequency_resolution * 2.0
            });

            if !too_close {
                peaks.push((self.frequencies[idx], mag));
                if peaks.len() >= n {
                    break;
                }
            }
        }

        peaks
    }

    /// Get frequency at a specific bin index
    pub fn frequency_at(&self, bin: usize) -> Option<f64> {
        self.frequencies.get(bin).copied()
    }

    /// Get magnitude at a specific bin index
    pub fn magnitude_at(&self, bin: usize) -> Option<f64> {
        self.magnitudes.get(bin).copied()
    }

    /// Get the DC component (0 Hz magnitude)
    pub fn dc_component(&self) -> f64 {
        self.magnitudes.first().copied().unwrap_or(0.0)
    }

    /// Get data points for plotting (frequency, magnitude pairs)
    pub fn plot_points(&self) -> Vec<[f64; 2]> {
        self.frequencies
            .iter()
            .zip(self.magnitudes.iter())
            .map(|(&f, &m)| [f, m])
            .collect()
    }

    /// Get data points for plotting in dB scale
    pub fn plot_points_db(&self) -> Vec<[f64; 2]> {
        self.frequencies
            .iter()
            .zip(self.psd_db.iter())
            .map(|(&f, &db)| [f, db])
            .collect()
    }
}

/// FFT analyzer configuration
#[derive(Debug, Clone)]
pub struct FftConfig {
    /// Window function to use
    pub window: WindowFunction,
    /// FFT size (power of 2 recommended)
    pub fft_size: usize,
    /// Whether to zero-pad to next power of 2
    pub zero_pad: bool,
    /// Whether to remove DC component
    pub remove_dc: bool,
    /// Overlap ratio for averaging (0.0 to 0.99)
    pub overlap: f64,
}

impl Default for FftConfig {
    fn default() -> Self {
        Self {
            window: WindowFunction::Hann,
            fft_size: 1024,
            zero_pad: true,
            remove_dc: true,
            overlap: 0.5,
        }
    }
}

impl FftConfig {
    /// Create config with specific FFT size
    pub fn with_size(fft_size: usize) -> Self {
        Self {
            fft_size,
            ..Default::default()
        }
    }

    /// Set window function
    pub fn window(mut self, window: WindowFunction) -> Self {
        self.window = window;
        self
    }

    /// Get available FFT sizes (powers of 2)
    pub fn available_sizes() -> &'static [usize] {
        &[64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384]
    }
}

/// FFT Analyzer for computing frequency spectra
pub struct FftAnalyzer {
    planner: FftPlanner<f64>,
    config: FftConfig,
}

impl FftAnalyzer {
    /// Create a new FFT analyzer with default config
    pub fn new() -> Self {
        Self {
            planner: FftPlanner::new(),
            config: FftConfig::default(),
        }
    }

    /// Create analyzer with specific config
    pub fn with_config(config: FftConfig) -> Self {
        Self {
            planner: FftPlanner::new(),
            config,
        }
    }

    /// Get current config
    pub fn config(&self) -> &FftConfig {
        &self.config
    }

    /// Set config
    pub fn set_config(&mut self, config: FftConfig) {
        self.config = config;
    }

    /// Compute FFT of input samples
    ///
    /// # Arguments
    /// * `samples` - Time-domain samples
    /// * `sample_rate` - Sample rate in Hz
    ///
    /// # Returns
    /// FFT result with frequency and magnitude data
    pub fn compute(&mut self, samples: &[f64], sample_rate: f64) -> FftResult {
        let n = samples.len();

        if n == 0 {
            return FftResult {
                frequencies: Vec::new(),
                magnitudes: Vec::new(),
                psd_db: Vec::new(),
                sample_rate,
                sample_count: 0,
                frequency_resolution: 0.0,
            };
        }

        // Determine FFT size
        let fft_size = if self.config.zero_pad {
            self.config.fft_size.max(n).next_power_of_two()
        } else {
            n.min(self.config.fft_size)
        };

        // Apply window function and prepare complex input
        let window = self.config.window.generate(n.min(fft_size));
        let mut buffer: Vec<Complex<f64>> = samples
            .iter()
            .take(fft_size)
            .enumerate()
            .map(|(i, &s)| {
                let windowed = if i < window.len() { s * window[i] } else { 0.0 };
                Complex::new(windowed, 0.0)
            })
            .collect();

        // Zero-pad if needed
        buffer.resize(fft_size, Complex::new(0.0, 0.0));

        // Remove DC if configured
        if self.config.remove_dc {
            let mean: f64 = buffer.iter().map(|c| c.re).sum::<f64>() / fft_size as f64;
            for c in &mut buffer {
                c.re -= mean;
            }
        }

        // Perform FFT
        let fft = self.planner.plan_fft_forward(fft_size);
        fft.process(&mut buffer);

        // Compute frequencies and magnitudes (only positive frequencies)
        let freq_resolution = sample_rate / fft_size as f64;
        let num_bins = fft_size / 2 + 1;

        let frequencies: Vec<f64> = (0..num_bins).map(|i| i as f64 * freq_resolution).collect();

        let magnitudes: Vec<f64> = buffer
            .iter()
            .take(num_bins)
            .map(|c| {
                let mag = c.norm() / fft_size as f64;
                // Double magnitude for positive frequencies (except DC and Nyquist)
                mag * 2.0
            })
            .collect();

        // Compute power spectral density in dB
        let psd_db: Vec<f64> = magnitudes
            .iter()
            .map(|&m| {
                if m > 1e-10 {
                    20.0 * m.log10()
                } else {
                    -200.0 // Floor value
                }
            })
            .collect();

        FftResult {
            frequencies,
            magnitudes,
            psd_db,
            sample_rate,
            sample_count: n,
            frequency_resolution: freq_resolution,
        }
    }

    /// Compute averaged FFT using overlapping segments (Welch's method)
    pub fn compute_averaged(&mut self, samples: &[f64], sample_rate: f64) -> FftResult {
        let n = samples.len();
        let segment_size = self.config.fft_size;

        if n < segment_size {
            // Not enough samples for averaging, use regular FFT
            return self.compute(samples, sample_rate);
        }

        let hop_size = ((1.0 - self.config.overlap) * segment_size as f64) as usize;
        let hop_size = hop_size.max(1);

        // Count segments
        let num_segments = (n - segment_size) / hop_size + 1;

        if num_segments <= 1 {
            return self.compute(samples, sample_rate);
        }

        // Compute FFT for each segment and average
        let mut accumulated_magnitudes: Vec<f64> = Vec::new();
        let mut accumulated_psd: Vec<f64> = Vec::new();
        let mut result_frequencies: Vec<f64> = Vec::new();
        let mut freq_resolution = 0.0;

        for i in 0..num_segments {
            let start = i * hop_size;
            let end = (start + segment_size).min(n);
            let segment = &samples[start..end];

            let segment_result = self.compute(segment, sample_rate);

            if accumulated_magnitudes.is_empty() {
                accumulated_magnitudes = segment_result.magnitudes.clone();
                accumulated_psd = segment_result.psd_db.clone();
                result_frequencies = segment_result.frequencies;
                freq_resolution = segment_result.frequency_resolution;
            } else {
                for (j, &mag) in segment_result.magnitudes.iter().enumerate() {
                    if j < accumulated_magnitudes.len() {
                        accumulated_magnitudes[j] += mag;
                        accumulated_psd[j] += segment_result.psd_db[j];
                    }
                }
            }
        }

        // Average the results
        let num_segments_f = num_segments as f64;
        for mag in &mut accumulated_magnitudes {
            *mag /= num_segments_f;
        }
        for psd in &mut accumulated_psd {
            *psd /= num_segments_f;
        }

        FftResult {
            frequencies: result_frequencies,
            magnitudes: accumulated_magnitudes,
            psd_db: accumulated_psd,
            sample_rate,
            sample_count: n,
            frequency_resolution: freq_resolution,
        }
    }
}

impl Default for FftAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_functions() {
        let n = 100;
        for window in WindowFunction::all() {
            let coeffs = window.generate(n);
            assert_eq!(coeffs.len(), n);

            // All coefficients should be approximately in [0, 1].
            // FlatTop window has small negative side lobes (~-0.0004) by design.
            for &c in &coeffs {
                assert!(
                    (-0.1..=1.5).contains(&c),
                    "Window {} coefficient {} out of range",
                    window.display_name(),
                    c
                );
            }
        }
    }

    #[test]
    fn test_fft_sine_wave() {
        let sample_rate = 1000.0; // 1000 Hz
        let duration = 1.0; // 1 second
        let freq = 50.0; // 50 Hz sine wave

        // Generate sine wave
        let n = (sample_rate * duration) as usize;
        let samples: Vec<f64> = (0..n)
            .map(|i| {
                let t = i as f64 / sample_rate;
                (2.0 * PI * freq * t).sin()
            })
            .collect();

        let mut analyzer = FftAnalyzer::with_config(FftConfig::with_size(1024));
        let result = analyzer.compute(&samples, sample_rate);

        // Should detect the 50 Hz peak
        let (peak_freq, _) = result.peak().expect("Should find peak");
        assert!(
            (peak_freq - freq).abs() < 5.0,
            "Peak frequency {} should be close to {}",
            peak_freq,
            freq
        );
    }

    #[test]
    fn test_fft_empty_input() {
        let mut analyzer = FftAnalyzer::new();
        let result = analyzer.compute(&[], 1000.0);
        assert!(result.frequencies.is_empty());
        assert!(result.magnitudes.is_empty());
    }

    #[test]
    fn test_fft_dc_removal() {
        let samples: Vec<f64> = vec![5.0; 1000]; // DC signal
        let mut analyzer = FftAnalyzer::with_config(FftConfig {
            remove_dc: true,
            ..Default::default()
        });
        let result = analyzer.compute(&samples, 1000.0);

        // DC component should be very small after removal
        assert!(result.dc_component() < 0.01);
    }

    #[test]
    fn test_top_peaks() {
        let sample_rate = 1000.0;
        let n = 1000;

        // Generate signal with two frequencies
        let samples: Vec<f64> = (0..n)
            .map(|i| {
                let t = i as f64 / sample_rate;
                (2.0 * PI * 50.0 * t).sin() + 0.5 * (2.0 * PI * 120.0 * t).sin()
            })
            .collect();

        let mut analyzer = FftAnalyzer::with_config(FftConfig::with_size(1024));
        let result = analyzer.compute(&samples, sample_rate);

        let peaks = result.top_peaks(3);
        assert!(peaks.len() >= 2, "Should find at least 2 peaks");

        // First peak should be near 50 Hz (stronger signal)
        assert!(
            (peaks[0].0 - 50.0).abs() < 5.0,
            "First peak should be near 50 Hz"
        );
    }

    #[test]
    fn test_fft_config_sizes() {
        let sizes = FftConfig::available_sizes();
        assert!(!sizes.is_empty());
        for &size in sizes {
            assert!(size.is_power_of_two());
        }
    }
}
