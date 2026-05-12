use std::sync::Arc;
use ringbuf::{HeapRb, traits::*, CachingProd, CachingCons};
use log::debug;

pub struct AudioMixer {
    mic_producer: CachingProd<Arc<HeapRb<f32>>>,
    sys_producer: CachingProd<Arc<HeapRb<f32>>>,
    mic_consumer: CachingCons<Arc<HeapRb<f32>>>,
    sys_consumer: CachingCons<Arc<HeapRb<f32>>>,
    sample_rate: u32,
}

impl AudioMixer {
    pub fn new(sample_rate: u32) -> Self {
        let mic_rb = Arc::new(HeapRb::<f32>::new(sample_rate as usize * 2));
        let sys_rb = Arc::new(HeapRb::<f32>::new(sample_rate as usize * 2));
        
        let (mic_producer, mic_consumer) = mic_rb.split();
        let (sys_producer, sys_consumer) = sys_rb.split();

        Self {
            mic_producer,
            sys_producer,
            mic_consumer,
            sys_consumer,
            sample_rate,
        }
    }

    pub fn push_mic_samples(&mut self, samples: &[f32]) {
        if self.mic_producer.push_slice(samples) < samples.len() {
            debug!("Mic buffer overflow in mixer");
        }
    }

    pub fn push_sys_samples(&mut self, samples: &[f32]) {
        if self.sys_producer.push_slice(samples) < samples.len() {
            debug!("System buffer overflow in mixer");
        }
    }

    pub fn pull_mixed_samples(&mut self) -> Vec<f32> {
        let mic_available = self.mic_consumer.occupied_len();
        let sys_available = self.sys_consumer.occupied_len();
        
        let count = std::cmp::max(mic_available, sys_available);
        if count == 0 {
            return Vec::new();
        }

        let mut mixed = vec![0.0f32; count];

        // Fill Mic
        for i in 0..mic_available {
            if let Some(s) = self.mic_consumer.try_pop() {
                mixed[i] += s;
            }
        }

        // Fill System
        for i in 0..sys_available {
            if let Some(s) = self.sys_consumer.try_pop() {
                mixed[i] += s;
            }
        }

        // Clamp
        for s in mixed.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }

        mixed
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}
