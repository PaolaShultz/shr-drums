//! Validated kit loading and a bounded, allocation-free render path.

mod engine;
mod package;
mod queue;
mod schema;

pub use engine::{DrumEngine, EngineError, StereoFrame};
pub use package::{load_package, PreparedKit, PreparedSample};
pub use queue::{event_queue, DrumEvent, EventReceiver, EventSender, PushError};
pub use schema::{
    EngineCompatibility, Envelope, FollowKeyRule, KitManifest, KitMetadata, KitProcessing,
    KitTuning, ManualTuning, ModeledParameters, MusicalMode, PitchClass, ProjectKey,
    SampleAssignment, SampleMetadata, TuningLimits, TuningMode, VoiceKind, VoiceManifest,
    KIT_FORMAT_VERSION,
};
