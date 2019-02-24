mod protos;

use crate::protos::language::{
    ErrorType, LanguageCode, LanguageRequest, SynthesizeResponse, TranslateResponse,
};
use crate::protos::language_grpc::{create_language, Language};
use futures::future::Future;
use grpcio::{self, Environment, RpcContext, RpcStatus, RpcStatusCode, ServerBuilder, UnarySink};
use rusoto_core::region::Region;
use rusoto_polly::{
    DescribeVoicesError, DescribeVoicesInput, Polly, PollyClient, SynthesizeSpeechError,
    SynthesizeSpeechInput,
};
use rusoto_translate::{Translate, TranslateClient, TranslateTextError, TranslateTextRequest};
use std::fmt;
use std::io::Read;
use std::sync::mpsc;
use std::sync::Arc;
use std::{io, thread};

impl fmt::Display for LanguageCode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let code_str = match self {
            LanguageCode::EN => "en",
            LanguageCode::ZH => "zh",
            LanguageCode::FR => "fr",
            LanguageCode::DE => "de",
            LanguageCode::PT => "pt",
            LanguageCode::ES => "es",
            _ => "",
        };
        write!(f, "{}", code_str)
    }
}

#[derive(Clone)]
struct LanguageService;

impl Language for LanguageService {
    fn translate(
        &mut self,
        ctx: RpcContext,
        req: LanguageRequest,
        sink: UnarySink<TranslateResponse>,
    ) {
        println!(
            "Got translation request\nSource Code: {}, Target Code: {}\nText: {}",
            req.source_language_code, req.target_language_code, req.text
        );

        // get translated text or error and create sending future from it
        let rpc_future =
            match self.translate_text(req.source_language_code, req.target_language_code, req.text)
            {
                Ok(translated_text) => {
                    let mut resp = TranslateResponse::new();
                    resp.translated_text = translated_text;
                    sink.success(resp)
                }
                Err(e) => {
                    let rpc_status = match e {
                        ErrorType::User => RpcStatus::new(RpcStatusCode::InvalidArgument, None),
                        _ => RpcStatus::new(RpcStatusCode::Internal, None),
                    };
                    sink.fail(rpc_status)
                }
            };
        ctx.spawn(rpc_future.map_err(|e| eprintln!("err replying: {}", e)));
    }

    fn synthesize(
        &mut self,
        ctx: RpcContext,
        req: LanguageRequest,
        sink: UnarySink<SynthesizeResponse>,
    ) {
        println!(
            "Got synthesis request\nSource Code: {}, Target Code: {}\nText: {}",
            req.source_language_code, req.target_language_code, req.text
        );

        if req.target_language_code == LanguageCode::UNKNOWN
            || req.source_language_code == LanguageCode::UNKNOWN
        {
            ctx.spawn(
                sink.fail(RpcStatus::new(RpcStatusCode::InvalidArgument, None))
                    .map_err(|e| eprintln!("err replying: {}", e)),
            );
            return;
        }

        // needed because aws expects - {ISO 639 language code}-{ISO 3166 country code}
        let language_code = match req.target_language_code {
            LanguageCode::ZH => "cmn-CN".to_owned(),
            LanguageCode::EN => "en-US".to_owned(),
            c @ _ => format!("{}-{}", c.to_string(), c.to_string().to_uppercase()),
        };
        let synthesize_result = self
            // first translate text
            .translate_text(req.source_language_code, req.target_language_code, req.text)
            .and_then(|translated_text| {
                self.get_voice_id(language_code)
                    // create rpc status from aws error -  propogate status down
                    .and_then(|voice_id| {
                        self.synthesize_speech(voice_id, translated_text)
                            .map(|audio_bytes| {
                                // store mp3 bytes
                                let mut resp = SynthesizeResponse::new();
                                resp.audio_bytes = audio_bytes;
                                resp
                            })
                    })
            })
            .map_err(|e| match e {
                ErrorType::User => RpcStatus::new(RpcStatusCode::InvalidArgument, None),
                _ => RpcStatus::new(RpcStatusCode::Internal, None),
            });

        // create sending future
        let send_future = match synthesize_result {
            Ok(resp) => sink.success(resp),
            Err(status) => sink.fail(status),
        }
        .map_err(|e| eprintln!("err replying: {}", e));

        ctx.spawn(send_future);
    }
}

impl LanguageService {
    fn translate_text(
        &self,
        source_language_code: LanguageCode,
        target_language_code: LanguageCode,
        text: String,
    ) -> Result<String, ErrorType> {
        let translate_client = TranslateClient::new(Region::default());

        // get translated text or error and create sending future from it
        translate_client
            .translate_text(TranslateTextRequest {
                source_language_code: source_language_code.to_string(),
                target_language_code: target_language_code.to_string(),
                text: text,
            })
            .sync()
            .map_err(|e| match e {
                TranslateTextError::InternalServer(_)
                | TranslateTextError::ServiceUnavailable(_)
                | TranslateTextError::HttpDispatch(_)
                | TranslateTextError::Unknown(_)
                | TranslateTextError::ParseError(_) => ErrorType::Internal,
                _ => ErrorType::User,
            })
            .map(|r| r.translated_text)
    }

    fn synthesize_speech(&self, voice_id: String, text: String) -> Result<Vec<u8>, ErrorType> {
        let polly_client = PollyClient::new(Region::default());

        // return audio bytes or error types
        polly_client
            .synthesize_speech(SynthesizeSpeechInput {
                text,
                voice_id,
                output_format: "mp3".to_owned(),
                ..SynthesizeSpeechInput::default()
            })
            .sync()
            .map_err(move |e| {
                println!("synthesize error: {}", e);

                match e {
                    SynthesizeSpeechError::HttpDispatch(_)
                    | SynthesizeSpeechError::Unknown(_)
                    | SynthesizeSpeechError::ServiceFailure(_)
                    | SynthesizeSpeechError::ParseError(_) => ErrorType::Internal,
                    _ => ErrorType::User,
                }
            })
            .map(|output| output.audio_stream.unwrap_or_default())
    }

    fn get_voice_id(&self, language_code: String) -> Result<String, ErrorType> {
        let polly_client = PollyClient::new(Region::default());
        polly_client
            // describe voices is used to get a list of avaialable voices for
            // the target language.
            .describe_voices(DescribeVoicesInput {
                language_code: Some(language_code),
                next_token: None,
            })
            .sync()
            // create rpc status from aws error -  propogate status down
            .map_err(|e| match e {
                DescribeVoicesError::InvalidNextToken(_)
                | DescribeVoicesError::Validation(_)
                | DescribeVoicesError::Credentials(_) => ErrorType::User,
                _ => ErrorType::Internal,
            })
            .map(|dv_output| {
                // just take first voice
                dv_output
                    .voices
                    .unwrap_or_default()
                    .iter()
                    .take(1)
                    .map(|voice| voice.id.clone().unwrap_or_else(|| "".to_owned()))
                    .next()
                    .unwrap_or_else(|| "".to_owned())
            })
    }
}
fn main() {
    let env = Arc::new(Environment::new(1));
    let service = create_language(LanguageService);
    let mut server = ServerBuilder::new(env)
        .register_service(service)
        .bind("0.0.0.0", 8081)
        .build()
        .unwrap();

    server.start();

    for &(ref host, port) in server.bind_addrs() {
        println!("listening on {}:{}", host, port);
    }

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        println!("Press ENTER to exit...");
        let _ = io::stdin().read(&mut [0]).unwrap();
        tx.send(())
    });
    let _ = rx.recv();
    let _ = server.shutdown().wait();
}
