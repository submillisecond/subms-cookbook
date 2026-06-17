//! `TsGorillaCodec` plugs the block into `subms-ts`'s `TsCodec` substrate, so
//! a `TsSeries<f64>` serializes to / from Gorilla bytes the same way it does
//! to JSON - and composes under wrappers like `subms-ts-gzip`.

use subms_ts::{TsCodec, TsPoint, TsSeries};

use crate::{TsBlockError, TsGorillaBlock};

#[derive(Clone, Debug, Default)]
pub struct TsGorillaCodec;

impl TsGorillaCodec {
    pub fn new() -> Self {
        Self
    }
}

impl TsCodec<f64> for TsGorillaCodec {
    type Error = TsBlockError;

    fn encode(&self, series: &TsSeries<f64>) -> Vec<u8> {
        let mut b = TsGorillaBlock::with_capacity(series.len() * 2 + 16);
        for p in series.iter() {
            b.append(p.ts, p.value);
        }
        b.bytes()
    }

    fn decode(&self, bytes: &[u8]) -> Result<TsSeries<f64>, Self::Error> {
        let block = TsGorillaBlock::from_bytes(bytes)?;
        let mut s = TsSeries::with_capacity(block.len());
        for TsPoint { ts, value } in block.iter() {
            // The block already enforced non-decreasing ts on the way in;
            // push cannot fail on a well-formed block.
            let _ = s.push(ts, value);
        }
        Ok(s)
    }

    fn format(&self) -> &str {
        "gorilla"
    }
}
