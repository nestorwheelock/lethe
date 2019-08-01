use rand::prelude::*;
use rand::SeedableRng;
use streaming_iterator::StreamingIterator;

#[derive(Debug)]
pub enum Stage {
    Fill { value: u8 },
    Random { seed: u64 }
}

#[derive(Debug)]
struct StreamState {
    total_size: u64,
    block_size: usize,
    position: u64,
    buf: Vec<u8>,
    current_block_size: usize,
    eof: bool
}

#[derive(Debug)]
enum StreamKind {
    Fill,
    Random { gen: StdRng }
}

#[derive(Debug)]
pub struct SanitizationStream {
    kind: StreamKind,
    state: StreamState
}

impl Stage {
    pub fn zero() -> Stage { 
        Stage::Fill { value: 0x00 } 
    }

    pub fn one() -> Stage { 
        Stage::Fill { value: 0xff } 
    }

    pub fn random_with_seed(seed: u64) -> Stage { 
        Stage::Random { seed }
    }

    pub fn random() -> Stage {
        Stage::random_with_seed(thread_rng().next_u64()) 
    }

    pub fn stream(&self, total_size: u64, block_size: usize) -> SanitizationStream {
        let (kind, buf) = match self {
            Stage::Fill { value } => {
                let buf = vec![*value; block_size];
                (StreamKind::Fill, buf)
            },
            Stage::Random { seed } => {
                let buf = vec![0; block_size];
                let gen = SeedableRng::seed_from_u64(*seed); 
                (StreamKind::Random { gen }, buf)
            }
        };

        let state = StreamState {
            total_size, 
            block_size, 
            position: 0, 
            buf,
            eof: false,
            current_block_size: 0
        };
        SanitizationStream { kind, state }
    }
}

impl StreamingIterator for SanitizationStream {
    type Item = [u8];

    fn advance(&mut self) {
        if !self.state.eof && self.state.position < self.state.total_size {
            let chunk_size = std::cmp::min(
                self.state.block_size as u64, 
                self.state.total_size - self.state.position) as usize;

            match &mut self.kind {
                StreamKind::Fill => (),
                StreamKind::Random { gen } =>
                    gen.fill_bytes(&mut self.state.buf)
            };

            self.state.current_block_size = chunk_size;
            self.state.position += chunk_size as u64;
        } else {
            self.state.eof = true;
        }
    }

    fn get(&self) -> Option<&Self::Item> {
        if !self.state.eof {
            Some(&self.state.buf[..self.state.current_block_size as usize])
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const TEST_SIZE: u64 = 10245;
    const TEST_BLOCK: usize = 256;

    #[test]
    fn test_stage_fill_behaves() {
        let mut data1 = create_test_vec();
        let mut stage = Stage::Fill { value: 0x33 };

        fill(&mut data1, &mut stage);
        assert!(data1.iter().find(|x| **x != 0x33).is_none());

        let mut data2 = create_test_vec();
        fill(&mut data2, &mut stage);

        assert_eq!(data1, data2);
    }

    #[test]
    fn test_stage_random_behaves() {
        let mut data1 = create_test_vec();
        let mut stage = Stage::random_with_seed(666);

        fill(&mut data1, &mut stage);

        assert_ne!(data1, create_test_vec());

        let unchanged = data1.iter().zip(create_test_vec().iter())
            .filter(|t| t.0 == t.1).count() as u64;

        assert!(unchanged < TEST_SIZE / 100); // allows for some edge cases

        let mut data2 = create_test_vec();
        fill(&mut data2, &mut stage);
        
        assert_eq!(data1, data2);

        let mut stage3 = Stage::random_with_seed(333);
        let mut data3 = create_test_vec();
        fill(&mut data3, &mut stage3);

        assert_ne!(data3, data2);
    }

    #[test]
    fn test_stage_random_entropy() {
        let mut data = create_test_vec();
        let mut stage = Stage::random_with_seed(666);
        fill(&mut data, &mut stage);

        let source_entropy = calculate_entropy(create_test_vec().as_ref());
        let stage_entropy = calculate_entropy(data.as_ref());

        assert!(stage_entropy > source_entropy);
        assert!(stage_entropy > 0.9);
    }

    fn create_test_vec() -> Vec<u8> {
        (0..TEST_SIZE).map(|x| (x % 256) as u8).collect()
    }

    fn fill(v: &mut Vec<u8>, stage: &mut Stage) -> () {
        let mut stream = stage.stream(TEST_SIZE, TEST_BLOCK);

        let mut position = 0;
        while let Some(chunk) = stream.next() {
            let chunk_size = chunk.len();
            v[position..position + chunk_size].clone_from_slice(chunk);
            position += chunk_size;
        }
    }

    fn calculate_entropy(v: &[u8]) -> f64 {
        use std::io::Write;
        use flate2::{write::ZlibEncoder, Compression};

        let mut e = ZlibEncoder::new(Vec::new(), Compression::best());
        e.write_all(v).unwrap();
        let compressed_bytes = e.finish();
        compressed_bytes.unwrap().len() as f64 / v.len() as f64
    }

}
