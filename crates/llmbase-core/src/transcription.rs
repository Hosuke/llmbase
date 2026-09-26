//! Supplied transcription boundary for evidence-bearing, read-only device asks.
//! Actual image/handwriting recognition requires an explicit future adapter.

use llmbase_model::{CancellationToken, ModelError};
use serde::{Deserialize, Serialize};

use crate::{
    Error, LibraryConfig, Result,
    ask::{AskAccess, AskEvent, AskModels, AskRequest, AskResponse, AskStream},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionInputKind {
    SuppliedTranscription,
    SyntheticTranscription,
    Image,
    Handwriting,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AnswerPreferences {
    pub short_answer: bool,
    pub max_sentences: Option<usize>,
    pub answer_language: Option<String>,
}

impl Default for AnswerPreferences {
    fn default() -> Self {
        Self {
            short_answer: true,
            max_sentences: None,
            answer_language: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptionRequest {
    pub version: u32,
    pub transcription_id: String,
    pub library_id: String,
    pub session_id: String,
    pub input_kind: TranscriptionInputKind,
    #[serde(default)]
    pub text: Option<String>,
    /// A recognized boundary shape only; no image is decoded or sent anywhere.
    #[serde(default)]
    pub image_base64: Option<String>,
    #[serde(default)]
    pub answer_preferences: AnswerPreferences,
    #[serde(default)]
    pub request_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionWritePolicy {
    None,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TranscriptionReceipt {
    pub version: u32,
    pub transcription_id: String,
    pub library_id: String,
    pub session_id: String,
    pub input_kind: TranscriptionInputKind,
    pub text_sha256: String,
    pub utf8_bytes: usize,
    pub unicode_scalars: usize,
    pub ocr_performed: bool,
    pub write_policy: TranscriptionWritePolicy,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PreparedTranscription {
    pub transcription: TranscriptionReceipt,
    pub question: AskRequest,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TranscriptionEvent {
    pub transcription_id: String,
    #[serde(flatten)]
    pub ask: AskEvent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TranscriptionResponse {
    pub transcription: TranscriptionReceipt,
    pub answer: AskResponse,
}

/// Pure preparation: no library creation, original-source mutation, or provider call.
pub fn prepare_transcription(
    expected_library_id: &str,
    request: &TranscriptionRequest,
) -> Result<PreparedTranscription> {
    if request.library_id != expected_library_id {
        return Err(Error::Scope);
    }
    if request.version != 1 {
        return Err(Error::Invalid("unsupported transcription version".into()));
    }
    crate::config::validate_id(&request.library_id)?;
    for (name, id) in [
        ("transcription_id", request.transcription_id.as_str()),
        ("session_id", request.session_id.as_str()),
    ]
    .into_iter()
    .chain(request.request_id.as_deref().map(|id| ("request_id", id)))
    {
        crate::config::validate_id(id).map_err(|_| {
            Error::Invalid(format!(
                "{name} must contain 1-128 ASCII letters, digits, hyphens or underscores"
            ))
        })?;
    }
    if request
        .image_base64
        .as_ref()
        .is_some_and(|image| image.len() > 8 * 1024 * 1024)
    {
        return Err(Error::Invalid(
            "transcription image input exceeds 8 MiB".into(),
        ));
    }
    if matches!(
        request.input_kind,
        TranscriptionInputKind::Image | TranscriptionInputKind::Handwriting
    ) {
        return Err(Error::Capability(
            "image/handwriting transcription requires a configured recognition adapter; supply transcript text instead".into(),
        ));
    }
    if request.image_base64.is_some() {
        return Err(Error::Invalid(
            "supplied transcription must contain text only".into(),
        ));
    }
    let text = request
        .text
        .as_deref()
        .ok_or_else(|| Error::Invalid("transcription text is required".into()))?;
    if text.trim().is_empty() || text.len() > 16_384 {
        return Err(Error::Invalid(
            "transcription text must be nonempty and at most 16384 UTF-8 bytes".into(),
        ));
    }
    if request
        .answer_preferences
        .max_sentences
        .is_some_and(|n| !(1..=100).contains(&n))
    {
        return Err(Error::Invalid("max_sentences must be from 1 to 100".into()));
    }
    if request
        .answer_preferences
        .answer_language
        .as_ref()
        .is_some_and(|language| {
            language.is_empty()
                || language.len() > 64
                || !language
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_".contains(c))
        })
    {
        return Err(Error::Invalid(
            "answer_language must be a bounded language/profile code".into(),
        ));
    }
    Ok(PreparedTranscription {
        transcription: TranscriptionReceipt {
            version: 1,
            transcription_id: request.transcription_id.clone(),
            library_id: request.library_id.clone(),
            session_id: request.session_id.clone(),
            input_kind: request.input_kind,
            text_sha256: crate::document::hash(text.as_bytes()),
            utf8_bytes: text.len(),
            unicode_scalars: text.chars().count(),
            ocr_performed: false,
            write_policy: TranscriptionWritePolicy::None,
        },
        question: AskRequest {
            question: text.to_owned(),
            library_id: request.library_id.clone(),
            session_id: Some(request.session_id.clone()),
            request_id: request.request_id.clone(),
            short_answer: request.answer_preferences.short_answer,
            max_sentences: request.answer_preferences.max_sentences,
            answer_language: request.answer_preferences.answer_language.clone(),
            ..AskRequest::default()
        },
    })
}

/// Owns the actual native ask stream; dropping it cancels that stream/provider.
pub struct TranscriptionStream {
    transcription: TranscriptionReceipt,
    ask: AskStream,
}

impl TranscriptionStream {
    pub(crate) fn from_ask(transcription: TranscriptionReceipt, ask: AskStream) -> Self {
        Self { transcription, ask }
    }

    pub fn transcription(&self) -> &TranscriptionReceipt {
        &self.transcription
    }

    pub fn request_id(&self) -> &str {
        self.ask.request_id()
    }

    pub fn cancel(&self) {
        self.ask.cancel();
    }

    pub async fn recv(&mut self) -> Option<TranscriptionEvent> {
        self.ask.recv().await.map(|ask| TranscriptionEvent {
            transcription_id: self.transcription.transcription_id.clone(),
            ask,
        })
    }

    pub async fn collect(self) -> Result<TranscriptionResponse> {
        Ok(TranscriptionResponse {
            transcription: self.transcription,
            answer: self.ask.collect().await?,
        })
    }
}

pub fn start_transcription_ask(
    config: LibraryConfig,
    models: AskModels,
    access: AskAccess,
    request: TranscriptionRequest,
    cancel: CancellationToken,
) -> Result<TranscriptionStream> {
    if cancel.is_cancelled() {
        return Err(ModelError::Cancelled.into());
    }
    let prepared = prepare_transcription(&config.library_id, &request)?;
    let ask = crate::ask::start_ask(config, models, access, prepared.question, cancel)?;
    Ok(TranscriptionStream::from_ask(prepared.transcription, ask))
}

pub async fn collect_transcription_ask(
    config: LibraryConfig,
    models: AskModels,
    access: AskAccess,
    request: TranscriptionRequest,
    cancel: CancellationToken,
) -> Result<TranscriptionResponse> {
    start_transcription_ask(config, models, access, request, cancel)?
        .collect()
        .await
}
