pub mod compaction;
pub mod eventlog;
pub mod identity;
pub mod materialize;
pub mod settings;

pub use eventlog::EventStore;
pub use identity::{
    data_paths, generate_secret_b64, load_pairings, load_peers, now_ms, save_pairings,
    save_peers, DataPaths, DesktopIdentity, OutboundPeer, Pairing, PairingFile, PeerFile,
};
pub use materialize::{materialize, MaterializedChat};
pub use settings::AppSettings;
