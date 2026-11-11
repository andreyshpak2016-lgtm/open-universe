//! CLI: рендерит сцены Open Universe в PNG.
//!
//! Запуск:
//!   cargo run -p ou-render --release -- <сцена> <ширина> <высота> [файл]
//! Сцены: ship, blackhole, asteroids, all
//!
//! Пример:
//!   cargo run -p ou-render --release -- ship 960 540 render-ship.png

use ou_render::{
    render, scene_asteroids, scene_black_hole, scene_from_states, scene_ship_and_planet, png,
    contact_sheet, scene_voxel_planet, scene_voxel_lod_compare,
};
use ou_protocol::{EntityState, InputFrame, Vec3};
use ou_sim::{det_cos, det_sin, World};
use std::collections::BTreeMap;
use std::time::Instant;

/// Прогнать симуляцию и вернуть кадры (позиции/углы) по тикам.
fn simulate_frames(ticks: u32, every: u32) -> Vec<Vec<EntityState>> {
    let mut world = World::new();
    world.spawn(1, Vec3::ZERO);
    world.spawn(2, Vec3::new(220.0, 0.0, 0.0));
    let mut frames = Vec::new();
    for t in 0..ticks {
        let mut inputs: BTreeMap<u32, InputFrame> = BTreeMap::new();
        // Игрок 1 маневрирует, игрок 2 летит рядом, чуть отставая.
        inputs.insert(
            1,
            InputFrame {
                seq: t,
                throttle: 1.0,
                yaw: (det_sin(t as f64 * 0.03) * 0.35) as f32,
                pitch: (det_cos(t as f64 * 0.05) * 0.10) as f32,
                fire: false,
            },
        );
        inputs.insert(
            2,
            InputFrame {
                seq: t,
                throttle: 0.8,
                yaw: (det_sin(t as f64 * 0.03 + 0.2) * 0.35) as f32,
                pitch: 0.0,
                fire: false,
            },
        );
        world.step(&inputs);
        if t % every == 0 {
            frames.push(world.snapshot_map().into_values().collect());
        }
    }
    frames
}

/// Отрисовать анимацию: несколько кадров из симуляции в один контактный лист.
fn render_animation(w: u32, h: u32, path: &str) {
    let ticks = 150;
    let every = 15; // 10 кадров
    let frames = simulate_frames(ticks, every);
    let t0 = Instant::now();
    let mut rendered = Vec::new();
    for states in &frames {
        let scene = scene_from_states(states, 1);
        rendered.push((w, h, render(&scene, w, h)));
    }
    let cols = 2;
    if let Some((sw, sh, data)) = contact_sheet(&rendered, cols) {
        match png::write_png(path, sw, sh, &data) {
            Ok(()) => println!(
                "  animation  {} кадров → {}×{}  {:.2} с  -> {path}",
                frames.len(),
                sw,
                sh,
                t0.elapsed().as_secs_f64()
            ),
            Err(e) => eprintln!("  animation: ошибка записи: {e}"),
        }
    }
}

fn render_scene(name: &str, w: u32, h: u32, path: &str) {
    let scene = match name {
        "ship" => scene_ship_and_planet(),
        "blackhole" => scene_black_hole(),
        "asteroids" => scene_asteroids(),
        "voxel" => scene_voxel_planet(),
        "lod" => scene_voxel_lod_compare(),
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

/// Отрисовать один кадр прямо из состояния симуляции.
fn render_sim_snapshot(w: u32, h: u32, path: &str) {
    let frames = simulate_frames(60, 60);
    let states = frames.last().expect("есть хотя бы один кадр");
    let scene = scene_from_states(states, 1);
    let t0 = Instant::now();
    let buf = render(&scene, w, h);
    match png::write_png(path, w, h, &buf) {
        Ok(()) => println!(
            "  sim        {w}×{h}  {:.2} с  ({} кораблей)  -> {path}",
            t0.elapsed().as_secs_f64(),
            states.len()
        ),
        Err(e) => eprintln!("  sim: ошибка записи: {e}"),
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
        "sim" => {
            render_sim_snapshot(w, h, "render-sim.png");
        }
        "animation" => {
            render_animation(w, h, "render-animation.png");
        }
        single => {
            let path = out.unwrap_or_else(|| format!("render-{single}.png"));
            render_scene(single, w, h, &path);
        }
    }
    println!("\nГотово. Кадры — это реальный вывод трассировщика лучей.");
}
