use std::thread;

use subms_seqlock::seqlock;

fn main() {
    let (mut writer, reader) = seqlock([10_000u64, 300, 10_001, 500, 0]);

    let strategy = thread::spawn(move || {
        let mut torn = 0u64;
        let mut last = 0u64;
        while last < 10_000 {
            let top = reader.read();
            if top[2] != top[0] + 1 || top[1] != 300 + top[4] % 1000 {
                torn += 1;
            }
            last = top[4];
        }
        torn
    });

    for i in 1..=10_000u64 {
        writer.write(|top| {
            top[0] = 10_000 + i % 5;
            top[2] = 10_001 + i % 5;
            top[1] = 300 + i % 1000;
            top[4] = i;
        });
    }

    let torn = strategy.join().unwrap();
    println!("writer published {} snapshots", writer.version());
    println!("reader saw {torn} torn snapshots");
}
