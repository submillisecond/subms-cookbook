use std::env;
use std::io;

use subms_lsm_tree::LsmTree;

fn main() -> io::Result<()> {
    let dir = env::temp_dir().join(format!("lsm-demo-{}", std::process::id()));
    println!("data dir: {}", dir.display());

    // 256-byte flush threshold so the demo actually rolls a few SSTables.
    let mut lsm = LsmTree::open(&dir, 256)?;

    lsm.put("AAPL", b"150.10")?;
    lsm.put("MSFT", b"320.55")?;
    lsm.put("GOOG", b"140.20")?;
    lsm.flush()?; // SSTable_0

    lsm.put("AAPL", b"150.42")?; // shadow older value
    lsm.delete("MSFT")?; // tombstone
    lsm.put("NVDA", b"900.00")?;
    lsm.flush()?; // SSTable_1

    for k in ["AAPL", "MSFT", "GOOG", "NVDA"] {
        match lsm.get(k)? {
            Some(v) => println!("{k} = {}", String::from_utf8_lossy(&v)),
            None => println!("{k} = <absent>"),
        }
    }
    println!("sstables: {}", lsm.sstable_count());
    Ok(())
}
