const GIGA_MAX_DIAGNOSTIC_CLASS_BYTES: usize = 128;

#[derive(Clone, Copy, Debug)]
pub(super) enum WorkerFailureKind {
    ClassifierOutput,
    Disabled,
    OllamaConfiguration,
    OllamaTransport,
    OllamaResponse,
    OllamaModelIdentity,
    ClassifierRequest,
    SourceVerification,
    LedgerUnavailable,
    SourceMissing,
    SourceAmbiguous,
    SourceHashMismatch,
    SourceWindowTooLarge,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct WorkerFailure {
    kind: WorkerFailureKind,
}

impl WorkerFailure {
    pub(super) const fn new(kind: WorkerFailureKind) -> Self {
        Self { kind }
    }

    pub(super) const fn class(self) -> &'static str {
        match self.kind {
            WorkerFailureKind::ClassifierOutput => "GigaClassifierOutputError",
            WorkerFailureKind::Disabled => "GigaClassifierDisabled",
            WorkerFailureKind::OllamaConfiguration => "GigaOllamaConfigurationError",
            WorkerFailureKind::OllamaTransport => "GigaOllamaTransportError",
            WorkerFailureKind::OllamaResponse => "GigaOllamaResponseError",
            WorkerFailureKind::OllamaModelIdentity => "GigaOllamaModelIdentityError",
            WorkerFailureKind::ClassifierRequest => "GigaClassifierRequestError",
            WorkerFailureKind::SourceVerification => "GigaSourceVerificationError",
            WorkerFailureKind::LedgerUnavailable => "GigaLedgerUnavailableError",
            WorkerFailureKind::SourceMissing => "GigaSourceMissingError",
            WorkerFailureKind::SourceAmbiguous => "GigaSourceAmbiguousError",
            WorkerFailureKind::SourceHashMismatch => "GigaSourceHashMismatchError",
            WorkerFailureKind::SourceWindowTooLarge => "GigaSourceWindowTooLargeError",
        }
    }

    pub(super) const fn retryable(self) -> bool {
        matches!(
            self.kind,
            WorkerFailureKind::ClassifierOutput
                | WorkerFailureKind::OllamaTransport
                | WorkerFailureKind::OllamaResponse
                | WorkerFailureKind::LedgerUnavailable
        )
    }
}

pub(super) fn domain_failure() -> WorkerFailure {
    WorkerFailure::new(WorkerFailureKind::ClassifierOutput)
}

pub(super) fn safe_error_class(value: Option<String>) -> Option<String> {
    value.filter(|class| {
        !class.is_empty()
            && class.len() <= GIGA_MAX_DIAGNOSTIC_CLASS_BYTES
            && class
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    })
}
