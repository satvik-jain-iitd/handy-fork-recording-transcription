use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::time::Duration;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Stream, StreamConfig};
use log::error;
use crossbeam_channel::Sender;

#[cfg(target_os = "macos")]
use screencapturekit::stream::{SCStream, SCStreamOutput, SCStreamDelegate, configuration::SCStreamConfiguration, content_filter::SCContentFilter, output_type::SCStreamOutputType};
#[cfg(target_os = "macos")]
use screencapturekit::shareable_content::SCShareableContent;
#[cfg(target_os = "macos")]
use screencapturekit::cm::CMSampleBuffer;

use crate::audio_toolkit::audio::{AudioMixer, FrameResampler};

pub struct MeetingRecorder {
    mic_stream: Option<Stream>,
    #[cfg(target_os = "windows")]
    sys_stream: Option<Stream>,
    #[cfg(target_os = "macos")]
    sck_stream: Option<SCStream>,
    
    shutdown: Arc<AtomicBool>,
    mixed_tx: Sender<Vec<f32>>,
}

#[cfg(target_os = "macos")]
struct AudioOutputHandler {
    mixer: Arc<Mutex<AudioMixer>>,
    resampler: Arc<Mutex<FrameResampler>>,
}

#[cfg(target_os = "macos")]
impl SCStreamOutput for AudioOutputHandler {
    fn did_output_sample_buffer(&self, sample_buffer: CMSampleBuffer, of_type: SCStreamOutputType) {
        if let SCStreamOutputType::Audio = of_type {
            if let Some(list) = sample_buffer.get_audio_buffer_list() {
                for i in 0..list.num_buffers() {
                    if let Some(buffer) = list.get(i) {
                        let data = buffer.data();
                        let f32_data: &[f32] = unsafe {
                            std::slice::from_raw_parts(data.as_ptr() as *const f32, data.len() / 4)
                        };
                        let mut resampler = self.resampler.lock().unwrap();
                        let mut mixer = self.mixer.lock().unwrap();
                        resampler.push(f32_data, &mut |frame: &[f32]| {
                            mixer.push_sys_samples(frame);
                        });
                    }
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
struct SCKDelegate;
#[cfg(target_os = "macos")]
impl SCStreamDelegate for SCKDelegate {}

impl MeetingRecorder {
    pub fn new(mixed_tx: Sender<Vec<f32>>) -> Self {
        Self {
            mic_stream: None,
            #[cfg(target_os = "windows")]
            sys_stream: None,
            #[cfg(target_os = "macos")]
            sck_stream: None,
            shutdown: Arc::new(AtomicBool::new(false)),
            mixed_tx,
        }
    }

    pub fn start(&mut self) -> Result<(), String> {
        let host = crate::audio_toolkit::get_cpal_host();
        
        let mixer = Arc::new(Mutex::new(AudioMixer::new(16000)));
        let mixer_mic = mixer.clone();
        
        // 1. Setup Mic Stream
        let mic_device = host.default_input_device()
            .ok_or("No default input device found")?;
        let mic_config: StreamConfig = mic_device.default_input_config()
            .map_err(|e| e.to_string())?.into();
        
        let mic_resampler = Arc::new(Mutex::new(FrameResampler::new(mic_config.sample_rate.0 as usize, 16000, Duration::from_millis(30))));
        
        let mic_stream = mic_device.build_input_stream(
            &mic_config,
            move |data: &[f32], _| {
                let mut resampler = mic_resampler.lock().unwrap();
                let mut mixer = mixer_mic.lock().unwrap();
                resampler.push(data, &mut |frame: &[f32]| {
                    mixer.push_mic_samples(frame);
                });
            },
            |err| error!("Mic stream error: {}", err),
            None
        ).map_err(|e| e.to_string())?;

        // 2. Setup System Stream
        #[cfg(target_os = "windows")]
        let sys_stream = {
            let sys_device = host.default_output_device()
                .ok_or("No default output device found")?;
            let sys_config: StreamConfig = sys_device.default_output_config()
                .map_err(|e| e.to_string())?.into();
            
            let mixer_sys = mixer.clone();
            let sys_resampler = Arc::new(Mutex::new(FrameResampler::new(sys_config.sample_rate.0 as usize, 16000, Duration::from_millis(30))));

            Some(sys_device.build_input_stream(
                &sys_config,
                move |data: &[f32], _| {
                    let mut resampler = sys_resampler.lock().unwrap();
                    let mut mixer = mixer_sys.lock().unwrap();
                    resampler.push(data, &mut |frame: &[f32]| {
                        mixer.push_sys_samples(frame);
                    });
                },
                |err| error!("System stream error: {}", err),
                None
            ).map_err(|e| e.to_string())?)
        };

        #[cfg(target_os = "macos")]
        let sck_stream = {
            let content = SCShareableContent::get().map_err(|e| e.to_string())?;
            let displays = content.displays();
            let display = displays.first().ok_or("No displays found")?;
            let filter = SCContentFilter::build()
                .display(display)
                .build();
            
            let config = SCStreamConfiguration::builder()
                .captures_audio(true)
                .excludes_current_process_audio(true)
                .build();

            let handler = AudioOutputHandler {
                mixer: mixer.clone(),
                resampler: Arc::new(Mutex::new(FrameResampler::new(48000, 16000, Duration::from_millis(30)))),
            };

            let mut stream = SCStream::new_with_delegate(&filter, &config, SCKDelegate);
            stream.add_output_handler(handler, SCStreamOutputType::Audio);
            stream.start_capture().map_err(|e| format!("Failed to start SCK stream: {:?}", e))?;
            Some(stream)
        };

        // 3. Start streams
        mic_stream.play().map_err(|e| e.to_string())?;
        #[cfg(target_os = "windows")]
        if let Some(ref s) = sys_stream {
            s.play().map_err(|e| e.to_string())?;
        }

        self.mic_stream = Some(mic_stream);
        #[cfg(target_os = "windows")]
        { self.sys_stream = sys_stream; }
        #[cfg(target_os = "macos")]
        { self.sck_stream = sck_stream; }

        // 4. Spawn Mixing Thread
        let shutdown = self.shutdown.clone();
        let mixed_tx = self.mixed_tx.clone();
        thread::spawn(move || {
            while !shutdown.load(Ordering::Relaxed) {
                let mixed = mixer.lock().unwrap().pull_mixed_samples();
                if !mixed.is_empty() {
                    if let Err(_) = mixed_tx.send(mixed) {
                        break;
                    }
                }
                thread::sleep(Duration::from_millis(10));
            }
        });

        Ok(())
    }

    pub fn stop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        self.mic_stream = None;
        #[cfg(target_os = "windows")]
        { self.sys_stream = None; }
        #[cfg(target_os = "macos")]
        if let Some(mut stream) = self.sck_stream.take() {
            let _ = stream.stop_capture();
        }
    }
}

// Safety: cpal Stream and SCStream can be wrapped to be Send/Sync for State storage
unsafe impl Send for MeetingRecorder {}
unsafe impl Sync for MeetingRecorder {}
