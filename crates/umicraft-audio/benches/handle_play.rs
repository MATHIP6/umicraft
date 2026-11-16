// SPDX-License-Identifier: MIT OR Apache-2.0

//! The cost of `AudioHandle::play` for the game thread, with room in the queue
//! and with a full one (the drop path).

use std::sync::Arc;

use criterion::{criterion_group, criterion_main, Criterion};
use umicraft_audio::cache::SoundCache;
use umicraft_audio::decode::DecodedSound;
use umicraft_audio::{channel, SoundRequest};

fn play(c: &mut Criterion) {
    let request = SoundRequest::new("dig.stone").at([1.0, 64.0, 2.0]);

    c.bench_function("audio/handle_play_empty_queue", |b| {
        // The receiver empties the queue after each call so every call enqueues.
        let (handle, receiver) = channel(1024);
        b.iter(|| {
            let sent = handle.play(request.clone());
            while receiver.try_recv().is_ok() {}
            sent
        });
    });

    c.bench_function("audio/handle_play_full_queue", |b| {
        let (handle, _receiver) = channel(16);
        while handle.play(request.clone()) {}
        b.iter(|| handle.play(request.clone()));
    });

    c.bench_function("audio/sound_cache_evict_churn", |b| {
        let dummy = DecodedSound {
            sample_rate: 44_100,
            frames: Arc::from(vec![[0.0, 0.0]; 10]),
        };
        b.iter(|| {
            // Holds 50 sounds of 80 bytes (4,000 bytes). Inserting 500 sounds forces 450 evictions.
            let mut cache = SoundCache::new(50 * 80);
            for i in 0..500 {
                let name = format!("sound_{i}");
                cache.insert(name, dummy.clone());
            }
        });
    });
}

criterion_group!(benches, play);
criterion_main!(benches);
