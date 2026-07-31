//! Validated kit loading and a bounded, allocation-free render path.

mod engine;
mod model;
mod package;
mod queue;
mod schema;

pub use engine::{DrumEngine, EngineDiagnostics, EngineError, StereoFrame};
pub use package::{load_package, PreparedKit, PreparedSample};
pub use queue::{event_queue, DrumEvent, EventReceiver, EventSender, PushError};
pub use schema::{
    AdvancedModel, BodyLayer, ClickLayer, DriveCurve, DriveStage, EngineCompatibility, Envelope,
    FilterMode, FollowKeyRule, KitManifest, KitMetadata, KitProcessing, KitTuning, ManualTuning,
    ModelAlgorithm, ModeledParameters, Modulation, MusicalMode, NoiseBurst, NoiseLayer,
    OscillatorShape, PitchClass, PitchEnvelope, ProjectKey, ResonantMode, SampleAssignment,
    SampleMetadata, SeededVariation, StereoModel, TuningLimits, TuningMode, VelocityResponse,
    VoiceKind, VoiceManifest, KIT_FORMAT_VERSION,
};
