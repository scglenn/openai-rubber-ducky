#[macro_use]
extern crate dotenv_codegen;
use core::time;
use leopard::{Leopard, LeopardBuilder};
use openai_api::api::CompletionArgs;
use pv_recorder::RecorderBuilder;
use std::io::{self, stdout, Write};
use std::process;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use tts_rust::tts::GTTSClient;

static RECORDING: AtomicBool = AtomicBool::new(false);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let access_key = dotenv!("ACCESS_KEY"); // AccessKey obtained from Picovoice Console (https://console.picovoice.ai/)
    let api_token = dotenv!("OPENAI_API_TOKEN"); // OpenAI API Token obtained from OpenAI Console (https://beta.openai.com/)

    let leopard: Leopard = LeopardBuilder::new()
        .access_key(access_key)
        .init()
        .expect("Unable to create Leopard");

    let recorder = RecorderBuilder::new()
        .device_index(0)
        .frame_length(512)
        .init()
        .expect("Failed to initialize pvrecorder");

    ctrlc::set_handler(|| {
        println!();
        process::exit(0);
    })
    .expect("Unable to setup signal handler");

    println!(">>> Press 'CTRL-C' to exit:");

    let narrator = GTTSClient {
        volume: 1.0,
        language: tts_rust::languages::Languages::English,
        tld: "com",
    };
    narrator.speak("Hello!");

    let mut input = String::new();

    loop {
        let mut audio_data = Vec::new();

        print!(">>> Press 'Enter' to start: ");

        stdout().flush().expect("Failed to flush");
        io::stdin()
            .read_line(&mut input)
            .expect("Failed to read input");
        RECORDING.store(true, Ordering::SeqCst);

        let leopard = leopard.clone();
        let recorder = recorder.clone();

        let transcript_handle = thread::spawn(move || {
            recorder.start().expect("Failed to start audio recording");
            while RECORDING.load(Ordering::SeqCst) {
                let mut pcm = vec![0; recorder.frame_length()];
                recorder.read(&mut pcm).expect("Failed to read audio frame");
                audio_data.extend_from_slice(&pcm);
            }
            recorder.stop().expect("Failed to stop audio recording");
            let leopard_transcript = leopard.process(&audio_data).unwrap();

            leopard_transcript
        });

        print!(">>> Recording ... Press 'Enter' to stop: ");
        stdout().flush().expect("Failed to flush");
        io::stdin()
            .read_line(&mut input)
            .expect("Failed to read input");
        RECORDING.store(false, Ordering::SeqCst);

        let leopard_transcript = transcript_handle.join().unwrap();
        println!("{}", &leopard_transcript.transcript);

        let client = openai_api::Client::new(&api_token);

        let initial_string = String::from("The following is a conversation with a sarcastic AI assistant. The assistant is helpful, creative, clever, and very friendly.\nHuman:") + &leopard_transcript.transcript + "?\nAI:";

        let test_string = String::from(initial_string);
        let args = CompletionArgs::builder()
            .prompt(test_string)
            .engine("text-davinci-003")
            .max_tokens(45)
            .stop(vec!["\n".into()])
            .top_p(0.5)
            .temperature(0.9)
            .frequency_penalty(0.5)
            .build()?;
        let completion = client.complete_prompt(args).await?;
        println!("Response: {}", &completion.choices[0].text);
        println!("Model used: {}", completion.model);

        // split result by each 99 characters
        let mut result = &completion.choices[0]
            .text
            .chars()
            .collect::<Vec<_>>()
            .chunks(99) // TODO: There is a 100 character limit on this TTS library
            .map(|c| c.iter().collect::<String>())
            .collect::<Vec<_>>();

        // for each element in result
        for phrase in result.iter() {
            // speak the element
            narrator.speak(phrase);
            println!("Result: {:?}", phrase);
        }

        let time = time::Duration::from_secs(2);
        thread::sleep(time);
    }

    //TODO: Next add pitch detection to make it real time
    // pitch-detection = "0.3.0"
    //show_audio_devices();
}

// TODO: I used this to find the index of the microphone
fn show_audio_devices() {
    let audio_devices = RecorderBuilder::new()
        .init()
        .expect("Failed to initialize pvrecorder")
        .get_audio_devices();
    match audio_devices {
        Ok(audio_devices) => {
            for (idx, device) in audio_devices.iter().enumerate() {
                println!("index: {}, device name: {:?}", idx, device);
            }
        }
        Err(err) => panic!("Failed to get audio devices: {}", err),
    };
}
