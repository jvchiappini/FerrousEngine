//! Font atlas dedupe and disk cache integration with the renderer.
//!
//! Registering one face twice must share a single atlas, and a cache installed
//! with `Renderer::set_font_cache` must store what it baked so the next
//! registration of the same face reads it back.  Every test skips cleanly when
//! no GPU adapter is available, mirroring the other headless tests.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use ferrous_font::{AtlasCache, BakedAtlas, FontKey, FontParser};
use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};

static GPU_LOCK: Mutex<()> = Mutex::new(());

const ROBOTO: &[u8] = include_bytes!("../../../assets/fonts/Roboto-Regular.ttf");

fn harness() -> Option<Pure2dHarness> {
    match Pure2dHarness::new(320, 200, DEFAULT_FORMAT) {
        Ok(harness) => Some(harness),
        Err(error) => {
            println!("[skip] headless GPU unavailable: {error}");
            None
        }
    }
}

/// A unique temporary cache directory for one test.
fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "ferrous-renderer-font-cache-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    root
}

#[test]
fn registering_the_same_face_twice_shares_one_atlas() {
    let _guard = GPU_LOCK.lock().unwrap();
    let Some(mut harness) = harness() else {
        return;
    };
    let first = harness
        .register_font(ROBOTO, ferrous_font::charset::latin_western())
        .expect("the bundled face registers");
    let second = harness
        .register_font(ROBOTO, ferrous_font::charset::latin_western())
        .expect("the second registration succeeds");
    assert_eq!(first, second, "the same face must return the same id");
    assert_eq!(harness.renderer().font_count_2d(), 1);

    // Freeing the shared entry frees both handles' atlas.
    harness.free_font(first);
    assert_eq!(harness.renderer().font_count_2d(), 0);
}

#[test]
fn different_character_sets_are_different_atlases() {
    let _guard = GPU_LOCK.lock().unwrap();
    let Some(mut harness) = harness() else {
        return;
    };
    let ascii = harness
        .register_font(ROBOTO, ferrous_font::charset::ascii())
        .expect("ascii");
    let western = harness
        .register_font(ROBOTO, ferrous_font::charset::latin_western())
        .expect("latin western");
    assert_ne!(ascii, western);
    assert_eq!(harness.renderer().font_count_2d(), 2);
}

#[test]
fn a_face_registered_with_a_cache_is_written_to_disk() {
    let _guard = GPU_LOCK.lock().unwrap();
    let Some(mut harness) = harness() else {
        return;
    };
    let root = temp_root("store");
    let cache = Arc::new(AtlasCache::new(&root));
    harness.renderer_mut().set_font_cache(Arc::clone(&cache));
    assert!(harness.renderer().font_cache().is_some());

    let chars = ferrous_font::charset::ascii();
    harness
        .register_font(ROBOTO, chars.clone())
        .expect("the bundled face registers");

    let key = FontKey::of(ROBOTO, &chars);
    let path = cache.path_for(key);
    assert!(path.exists(), "the bake must be cached at {path:?}");
    let stored = BakedAtlas::decode(&std::fs::read(&path).unwrap()).expect("a valid cache file");
    assert_eq!(stored.key, key);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_corrupt_cache_file_is_replaced_by_a_fresh_bake() {
    let _guard = GPU_LOCK.lock().unwrap();
    let Some(mut harness) = harness() else {
        return;
    };
    let root = temp_root("corrupt");
    let cache = Arc::new(AtlasCache::new(&root));
    harness.renderer_mut().set_font_cache(Arc::clone(&cache));

    let chars = ferrous_font::charset::ascii();
    let key = FontKey::of(ROBOTO, &chars);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(cache.path_for(key), b"not an atlas").unwrap();

    let id = harness
        .register_font(ROBOTO, chars)
        .expect("a corrupt cache entry must not fail registration");
    assert_eq!(harness.renderer().font_count_2d(), 1);
    assert!(harness.renderer().font_2d(id).is_some());
    assert!(
        BakedAtlas::decode(&std::fs::read(cache.path_for(key)).unwrap()).is_ok(),
        "the corrupt file is rebuilt"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn registering_an_already_baked_atlas_shares_the_same_entry() {
    let _guard = GPU_LOCK.lock().unwrap();
    let Some(mut harness) = harness() else {
        return;
    };
    let chars = ferrous_font::charset::ascii();
    let parser = FontParser::new(ROBOTO.to_vec()).expect("the bundled face parses");
    let baked = BakedAtlas::build(&parser, &chars).expect("the atlas bakes");

    // A worker may bake the atlas and the frame thread only uploads it.
    let first = harness
        .renderer_mut()
        .register_baked_font_2d(baked.clone())
        .expect("the atlas registers");
    let second = harness
        .renderer_mut()
        .register_baked_font_2d(baked)
        .expect("the second registration succeeds");
    assert_eq!(first, second, "the same key must share one atlas");
    assert_eq!(harness.renderer().font_count_2d(), 1);

    // Registering the same face by bytes finds the same entry.
    let by_bytes = harness.register_font(ROBOTO, chars).expect("ascii");
    assert_eq!(by_bytes, first);
    assert_eq!(harness.renderer().font_count_2d(), 1);
}
