/// One candidate driver: its network parameters plus breeding bookkeeping.
#[derive(Clone, Debug, PartialEq)]
pub struct Genome {
    pub id: u64,
    pub params: Vec<f32>,
    /// Its own mutation step size (self-adaptive).
    pub sigma: f64,
    pub parents: [Option<u64>; 2],
    /// Generation it was created in.
    pub born: u32,
}
