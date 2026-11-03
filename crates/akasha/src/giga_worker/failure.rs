const GIGA_MAX_DIAGNOSTIC_CLASS_BYTES: usize = 128;

#[derive(Clone, Copy, Debug)]
pub(super) enum WorkerFailureKind {
    ClassifierOutput,
    TypesafeConfiguration,
    TypesafeTransport,
    TypesafeResponse,
    ClassifierRequest,
    SourceVerification,
    LedgerUnavailable,
    SourceMissing,
    SourceAmbiguous,
    SourceHashMismatch,
    SourceWindowTooLarge,
    ResonanceStorage,
    PrivacyRefused,
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
            WorkerFailureKind::TypesafeConfiguration => "GigaTypesafeConfigurationError",
            WorkerFailureKind::TypesafeTransport => "GigaTypesafeTransportError",
            WorkerFailureKind::TypesafeResponse => "GigaTypesafeResponseError",
            WorkerFailureKind::ClassifierRequest => "GigaClassifierRequestError",
            WorkerFailureKind::SourceVerification => "GigaSourceVerificationError",
            WorkerFailureKind::LedgerUnavailable => "GigaLedgerUnavailableError",
            WorkerFailureKind::SourceMissing => "GigaSourceMissingError",
            WorkerFailureKind::SourceAmbiguous => "GigaSourceAmbiguousError",
            WorkerFailureKind::SourceHashMismatch => "GigaSourceHashMismatchError",
            WorkerFailureKind::SourceWindowTooLarge => "GigaSourceWindowTooLargeError",
            WorkerFailureKind::ResonanceStorage => "GigaResonanceStorageError",
            WorkerFailureKind::PrivacyRefused => "GigaPrivacyRefused",
        }
    }

    pub(super) const fn retryable(self) -> bool {
        matches!(
            self.kind,
            WorkerFailureKind::ClassifierOutput
                | WorkerFailureKind::TypesafeConfiguration
                | WorkerFailureKind::TypesafeTransport
                | WorkerFailureKind::TypesafeResponse
                | WorkerFailureKind::LedgerUnavailable
                | WorkerFailureKind::ResonanceStorage,
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
