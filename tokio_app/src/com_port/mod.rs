pub mod decode;
pub mod discover;
pub mod model;
pub mod reader;
pub mod serial;
pub mod sync;

pub use discover::{find_rcm_port, find_rcm_port_among, list_port_names, probe_port, ProbeResult};
pub use model::{ChannelType, DataPacket, Frame};
pub use reader::{decode_dump, ComPortReader, DumpStats};
pub use serial::SerialConfig;
pub use sync::FrameSynchronizer;
