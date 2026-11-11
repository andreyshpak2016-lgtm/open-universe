//! CLI: рендерит сцены Open Universe в PNG.
//!
//! Запуск:
//!   cargo run -p ou-render --release -- <сцена> <ширина> <высота> [файл]
//! Сцены: ship, blackhole, asteroids, all
//!
//! Пример:
//!   cargo run -p ou-render --release -- ship 960 540 render-ship.png

use ou_render::{render, scene_asteroids, scene_black_hole, scene_ship_and_planet, png};
use std::time::Instant;

fn render_scene(name: &str, w: u32, h: u32, path: &str) {
    let scene = match name {
        "ship" => scene_ship_and_planet(),
        "blackhole" => scene_black_hole(),
        "asteroids" => scene_asteroids(),
        _ => {
            eprintln!("неизвестная сцена: {name}");
            return;
        }
    };
    let t0 = Instant::now();
    let buf = render(&scene, w, h);
    let dt = t0.elapsed();
    match png::write_png(path, w, h, &buf) {
        Ok(()) => {
            let px = (w as u64) * (h as u64);
            println!(
                "  {name:<10} {w}×{h}  {:.2} с  ({:.0} пикс/с)  -> {path}",
                dt.as_secs_f64(),
                px as f64 / dt.as_secs_f64()
            );
        }
        Err(e) => eprintln!("  {name}: ошибка записи: {e}"),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let scene = args.get(1).cloned().unwrap_or_else(|| "all".to_string());
    let w: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(640);
    let h: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(360);
    let out = args.get(4).cloned();

    println!("Open Universe — софтверный рендерер (CPU raytracing)\n");

    match scene.as_str() {
        "all" => {
            render_scene("ship", w, h, "render-ship.png");
            render_scene("blackhole", w, h, "render-blackhole.png");
            render_scene("asteroids", w, h, "render-asteroids.png");
        }
        single => {
            let path = out.unwrap_or_else(|| format!("render-{single}.png"));
            render_scene(single, w, h, &path);
        }
    }
    println!("\nГотово. Кадры — это реальный вывод трассировщика лучей.");
}
