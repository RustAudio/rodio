//! Comprehensive integration tests for noise generators.
//! Tests deterministic behavior, range properties, and seeking capabilities.

#![cfg(feature = "noise")]

use rand::rngs::SmallRng;
use rand_distr::num_traits::Signed;
use rodio::source::{
    blue, brownian, gaussian_white, pink, triangular_white, velvet, violet, white, BlueNoise,
    BrownianNoise, GaussianWhiteNoise, NoiseGenerator, PinkNoise, Source, TriangularWhiteNoise,
    VelvetNoise, VioletNoise, WhiteNoise,
};
use std::time::Duration;

#[test]
fn test_white_noise_with_seed_deterministic() {
    let sample_rate = 44100;
    let seed = 12345u64;

    let mut noise1 = WhiteNoise::<SmallRng>::new_with_seed(sample_rate, seed);
    let mut noise2 = WhiteNoise::<SmallRng>::new_with_seed(sample_rate, seed);

    // First 1000 samples should be identical
    for i in 0..1000 {
        let sample1 = noise1.next().unwrap();
        let sample2 = noise2.next().unwrap();
        assert_eq!(
            sample1, sample2,
            "White noise sample {} differs between seeded generators",
            i
        );
        // White noise should be in [-1.0, 1.0]
        assert!(
            sample1 >= -1.0 && sample1 <= 1.0,
            "White noise sample out of range: {}",
            sample1
        );
    }
}

#[test]
fn test_gaussian_white_noise_with_seed_deterministic() {
    let sample_rate = 44100;
    let seed = 67890u64;

    let mut noise1 = GaussianWhiteNoise::<SmallRng>::new_with_seed(sample_rate, seed);
    let mut noise2 = GaussianWhiteNoise::<SmallRng>::new_with_seed(sample_rate, seed);

    // Test deterministic behavior and statistical properties
    let mut samples = Vec::new();
    for i in 0..1000 {
        let sample1 = noise1.next().unwrap();
        let sample2 = noise2.next().unwrap();
        assert_eq!(
            sample1, sample2,
            "Gaussian white noise sample {} differs between seeded generators",
            i
        );
        samples.push(sample1);
    }

    // Test mean/std_dev accessor methods
    assert_eq!(noise1.mean(), 0.0);
    assert!((noise1.std_dev() - (1.0 / 3.0)).abs() < f32::EPSILON);

    // Check that most samples are within reasonable bounds (3-sigma rule)
    let out_of_bounds = samples.iter().filter(|&&s| s < -1.0 || s > 1.0).count();
    let outlier_percentage = (out_of_bounds as f64 / samples.len() as f64) * 100.0;
    assert!(
        outlier_percentage < 1.0,
        "Too many Gaussian samples outside [-1.0, 1.0]: {:.2}%",
        outlier_percentage
    );
}

#[test]
fn test_triangular_white_noise_with_seed_deterministic() {
    let sample_rate = 48000;
    let seed = 11111u64;

    let mut noise1 = TriangularWhiteNoise::<SmallRng>::new_with_seed(sample_rate, seed);
    let mut noise2 = TriangularWhiteNoise::<SmallRng>::new_with_seed(sample_rate, seed);

    for i in 0..500 {
        let sample1 = noise1.next().unwrap();
        let sample2 = noise2.next().unwrap();
        assert_eq!(
            sample1, sample2,
            "Triangular white noise sample {} differs between seeded generators",
            i
        );
        // Triangular white noise should be in [-1.0, 1.0]
        assert!(
            sample1 >= -1.0 && sample1 <= 1.0,
            "Triangular white noise sample out of range: {}",
            sample1
        );
    }
}

#[test]
fn test_pink_noise_with_seed_deterministic() {
    let sample_rate = 44100;
    let seed = 22222u64;

    let mut noise1 = PinkNoise::<SmallRng>::new_with_seed(sample_rate, seed);
    let mut noise2 = PinkNoise::<SmallRng>::new_with_seed(sample_rate, seed);

    for i in 0..1000 {
        let sample1 = noise1.next().unwrap();
        let sample2 = noise2.next().unwrap();
        assert_eq!(
            sample1, sample2,
            "Pink noise sample {} differs between seeded generators",
            i
        );
    }
}

#[test]
fn test_blue_noise_with_seed_deterministic() {
    let sample_rate = 44100;
    let seed = 33333u64;

    let mut noise1 = BlueNoise::<SmallRng>::new_with_seed(sample_rate, seed);
    let mut noise2 = BlueNoise::<SmallRng>::new_with_seed(sample_rate, seed);

    for i in 0..1000 {
        let sample1 = noise1.next().unwrap();
        let sample2 = noise2.next().unwrap();
        assert_eq!(
            sample1, sample2,
            "Blue noise sample {} differs between seeded generators",
            i
        );
    }
}

#[test]
fn test_violet_noise_with_seed_deterministic() {
    let sample_rate = 44100;
    let seed = 44444u64;

    let mut noise1 = VioletNoise::<SmallRng>::new_with_seed(sample_rate, seed);
    let mut noise2 = VioletNoise::<SmallRng>::new_with_seed(sample_rate, seed);

    for i in 0..1000 {
        let sample1 = noise1.next().unwrap();
        let sample2 = noise2.next().unwrap();
        assert_eq!(
            sample1, sample2,
            "Violet noise sample {} differs between seeded generators",
            i
        );
    }
}

#[test]
fn test_brownian_noise_with_seed_deterministic() {
    let sample_rate = 48000;
    let seed = 55555u64;

    let mut noise1 = BrownianNoise::<SmallRng>::new_with_seed(sample_rate, seed);
    let mut noise2 = BrownianNoise::<SmallRng>::new_with_seed(sample_rate, seed);

    for i in 0..1000 {
        let sample1 = noise1.next().unwrap();
        let sample2 = noise2.next().unwrap();
        assert_eq!(
            sample1, sample2,
            "Brownian noise sample {} differs between seeded generators",
            i
        );
    }
}

#[test]
fn test_velvet_noise_with_seed_deterministic() {
    let sample_rate = 44100;
    let seed = 66666u64;

    let mut noise1 = VelvetNoise::<SmallRng>::new_with_seed(sample_rate, seed);
    let mut noise2 = VelvetNoise::<SmallRng>::new_with_seed(sample_rate, seed);

    for i in 0..1000 {
        let sample1 = noise1.next().unwrap();
        let sample2 = noise2.next().unwrap();
        assert_eq!(
            sample1, sample2,
            "Velvet noise sample {} differs between seeded generators",
            i
        );
        // Velvet noise should be 0.0, 1.0, or -1.0
        assert!(
            sample1 == 0.0 || sample1 == 1.0 || sample1 == -1.0,
            "Velvet noise sample has invalid value: {}",
            sample1
        );
    }
}

#[test]
fn test_convenience_functions_work() {
    // Test that all convenience functions create working generators
    let sample_rate = 44100;

    let mut white = white(sample_rate);
    let mut pink = pink(sample_rate);
    let mut blue = blue(sample_rate);
    let mut violet = violet(sample_rate);
    let mut brownian = brownian(sample_rate);
    let mut velvet = velvet(sample_rate);
    let mut gaussian = gaussian_white(sample_rate);
    let mut triangular = triangular_white(sample_rate);

    // Each should produce valid samples
    assert!(white.next().is_some());
    assert!(pink.next().is_some());
    assert!(blue.next().is_some());
    assert!(violet.next().is_some());
    assert!(brownian.next().is_some());
    assert!(velvet.next().is_some());
    assert!(gaussian.next().is_some());
    assert!(triangular.next().is_some());
}

#[test]
fn test_seeking_behavior() {
    let sample_rate = 44100;
    let seed = 77777u64;

    // Test seekable generators
    let mut white = WhiteNoise::<SmallRng>::new_with_seed(sample_rate, seed);
    let mut gaussian = GaussianWhiteNoise::<SmallRng>::new_with_seed(sample_rate, seed);
    let mut triangular = TriangularWhiteNoise::<SmallRng>::new_with_seed(sample_rate, seed);
    let mut violet = VioletNoise::<SmallRng>::new_with_seed(sample_rate, seed);

    // Seeking should work for stateless/seekable generators
    assert!(white.try_seek(Duration::from_secs(1)).is_ok());
    assert!(gaussian.try_seek(Duration::from_secs(1)).is_ok());
    assert!(triangular.try_seek(Duration::from_secs(1)).is_ok());
    assert!(violet.try_seek(Duration::from_secs(1)).is_ok());

    // Non-seekable generators should error
    let mut pink = PinkNoise::<SmallRng>::new_with_seed(sample_rate, seed);
    let mut blue = BlueNoise::<SmallRng>::new_with_seed(sample_rate, seed);
    let mut brownian = BrownianNoise::<SmallRng>::new_with_seed(sample_rate, seed);
    let mut velvet = VelvetNoise::<SmallRng>::new_with_seed(sample_rate, seed);

    assert!(pink.try_seek(Duration::from_secs(1)).is_err());
    assert!(blue.try_seek(Duration::from_secs(1)).is_err());
    assert!(brownian.try_seek(Duration::from_secs(1)).is_err());
    assert!(velvet.try_seek(Duration::from_secs(1)).is_err());
}

#[test]
fn test_source_trait_properties() {
    let sample_rate = 44100;

    let white = white(sample_rate);
    let pink = pink(sample_rate);
    let violet = violet(sample_rate);

    // All noise generators should be mono, infinite duration
    assert_eq!(white.channels(), 1);
    assert_eq!(pink.channels(), 1);
    assert_eq!(violet.channels(), 1);

    assert_eq!(white.sample_rate(), sample_rate);
    assert_eq!(pink.sample_rate(), sample_rate);
    assert_eq!(violet.sample_rate(), sample_rate);

    assert_eq!(white.total_duration(), None);
    assert_eq!(pink.total_duration(), None);
    assert_eq!(violet.total_duration(), None);

    assert_eq!(white.current_span_len(), None);
    assert_eq!(pink.current_span_len(), None);
    assert_eq!(violet.current_span_len(), None);
}

#[test]
fn test_different_seeds_produce_different_outputs() {
    let sample_rate = 44100;

    // Test that different seeds produce different outputs
    let mut noise1 = WhiteNoise::<SmallRng>::new_with_seed(sample_rate, 12345);
    let mut noise2 = WhiteNoise::<SmallRng>::new_with_seed(sample_rate, 54321);

    let samples1: Vec<f32> = (0..100).map(|_| noise1.next().unwrap()).collect();
    let samples2: Vec<f32> = (0..100).map(|_| noise2.next().unwrap()).collect();

    // Should not be identical (extremely unlikely with different seeds)
    assert_ne!(
        samples1, samples2,
        "Different seeds should produce different outputs"
    );
}

#[test]
fn test_velvet_noise_sparsity() {
    let sample_rate = 44100;
    let seed = 88888u64;

    let mut velvet = VelvetNoise::<SmallRng>::new_with_seed(sample_rate, seed);

    // Count non-zero samples over a period
    let test_samples = sample_rate; // 1 second worth
    let mut non_zero_count = 0;

    for _ in 0..test_samples {
        let sample = velvet.next().unwrap();
        if sample != 0.0 {
            non_zero_count += 1;
        }
    }

    // Should have roughly 2000 impulses per second (default density)
    let density = non_zero_count as f32;
    assert!(
        density > 1500.0 && density < 2500.0,
        "Velvet noise density out of expected range: {} impulses/second",
        density
    );
}

#[test]
fn test_brownian_noise_low_frequency_character() {
    let sample_rate = 44100;
    let seed = 99999u64;

    let mut brownian = BrownianNoise::<SmallRng>::new_with_seed(sample_rate, seed);

    // Brownian noise should have low-frequency character
    // This means consecutive samples should be correlated (not independent like white noise)
    let samples: Vec<f32> = (0..1000).map(|_| brownian.next().unwrap()).collect();

    // Calculate correlation between consecutive samples
    let mut correlation_sum = 0.0;
    for i in 0..samples.len() - 1 {
        correlation_sum += samples[i] * samples[i + 1];
    }
    let avg_correlation: f32 = correlation_sum / (samples.len() - 1) as f32;

    // Brownian noise should have positive correlation between consecutive samples
    // (unlike white noise which should have correlation near zero)
    assert!(
        avg_correlation.is_positive(),
        "Brownian noise should have correlation between consecutive samples, got: {}",
        avg_correlation
    );
}
