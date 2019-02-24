mod protos;

use crate::protos::language::{LanguageCode, LanguageRequest};
use crate::protos::language_grpc::LanguageClient;
use grpcio::{ChannelBuilder, EnvBuilder};
use std::fs;
use std::io;
use std::sync::Arc;

// Stage represents the different stages a user can be in the application.
// They are either picking an operation, picking a src or target language, or
// inputing the text to translate or synthesize
#[derive(Debug)]
enum Stage {
    OPERATION,
    SRC,
    TARGET,
    TEXT,
}

fn get_languages() -> Vec<&'static str> {
    vec![
        "Mandarin",
        "English",
        "French",
        "German",
        "Spanish",
        "Portugese",
    ]
}

fn get_ops() -> Vec<&'static str> {
    vec!["Translate", "Synthesize"]
}

fn main() {
    let env = Arc::new(EnvBuilder::new().build());
    let ch = ChannelBuilder::new(env).connect("localhost:8081");
    let client = LanguageClient::new(ch);

    let mut req = LanguageRequest::new();
    let mut stage = Stage::OPERATION;
    let mut op = "";
    let mut count = 0;

    println!("Hello! Welcome to the translation service. Press ENTER to exit...");
    loop {
        let mut input = String::new();
        match stage {
            Stage::OPERATION => {
                println!(
                    "\nWhat would you like to do?\n{}",
                    get_ops()
                        .iter()
                        .enumerate()
                        .map(|(idx, op)| format!("{} {}", idx + 1, op))
                        .collect::<Vec<String>>()
                        .join("\n")
                );
                io::stdin()
                    .read_line(&mut input)
                    .expect("error reading line");
                match input.trim() {
                    "1" => {
                        stage = Stage::SRC;
                        op = "translate";
                    }
                    "2" => {
                        stage = Stage::SRC;
                        op = "synthesize";
                    }
                    _ => {}
                }
            }
            Stage::SRC => {
                println!(
                    "\nWhat is the source language?\n{}",
                    get_languages()
                        .iter()
                        .enumerate()
                        .map(|(idx, op)| format!("{} {}", idx + 1, op))
                        .collect::<Vec<String>>()
                        .join("\n")
                );
                io::stdin()
                    .read_line(&mut input)
                    .expect("error reading line");
                match input.trim() {
                    "1" => {
                        req.source_language_code = LanguageCode::ZH;
                    }
                    "2" => {
                        req.source_language_code = LanguageCode::EN;
                    }
                    "3" => {
                        req.source_language_code = LanguageCode::FR;
                    }
                    "4" => {
                        req.source_language_code = LanguageCode::DE;
                    }
                    "5" => {
                        req.source_language_code = LanguageCode::ES;
                    }
                    "6" => {
                        req.source_language_code = LanguageCode::PT;
                    }
                    _ => {
                        req.source_language_code = LanguageCode::UNKNOWN;
                    }
                };
                if req.source_language_code != LanguageCode::UNKNOWN {
                    stage = Stage::TARGET;
                }
            }
            Stage::TARGET => {
                println!(
                    "\nWhat is the target language?\n{}",
                    get_languages()
                        .iter()
                        .enumerate()
                        .map(|(idx, op)| format!("{} {}", idx + 1, op))
                        .collect::<Vec<String>>()
                        .join("\n")
                );
                io::stdin()
                    .read_line(&mut input)
                    .expect("error reading line");
                match input.trim() {
                    "1" => {
                        req.target_language_code = LanguageCode::ZH;
                    }
                    "2" => {
                        req.target_language_code = LanguageCode::EN;
                    }
                    "3" => {
                        req.target_language_code = LanguageCode::FR;
                    }
                    "4" => {
                        req.target_language_code = LanguageCode::DE;
                    }
                    "5" => {
                        req.target_language_code = LanguageCode::ES;
                    }
                    "6" => {
                        req.target_language_code = LanguageCode::PT;
                    }
                    _ => {
                        req.target_language_code = LanguageCode::UNKNOWN;
                    }
                };
                if req.target_language_code != LanguageCode::UNKNOWN {
                    stage = Stage::TEXT;
                }
            }
            Stage::TEXT => {
                println!("Enter the text to translate or synthesize");
                io::stdin()
                    .read_line(&mut input)
                    .expect("error reading line");
                req.text = input.clone();
                if op == "translate" {
                    println!("Translating: {}", req.text);
                    let resp = client.translate(&req);
                    if let Ok(resp) = resp {
                        println!("Translated Text: {}\n", resp.translated_text);
                        stage = Stage::OPERATION;
                    } else {
                        eprintln!("Not able to translate text");
                    }
                } else {
                    println!("Synthesizing: {}", req.text);
                    match client.synthesize(&req) {
                        Ok(resp) => {
                            count += 1;
                            fs::write(format!("syn-{}.mp3", count), resp.audio_bytes)
                                .expect("failed to write bytes");
                            println!(
                                "Synthesized Text, audio bytes written to syn-{}.mp3\n",
                                count
                            );
                            stage = Stage::OPERATION;
                        }
                        Err(e) => {
                            eprintln!("Not able to synthesize text: {}", e);
                        }
                    };
                }
            }
        }
        // quit if nothing is submitted
        if input.trim().is_empty() {
            println!("Bye!");
            break;
        }
    }
}
