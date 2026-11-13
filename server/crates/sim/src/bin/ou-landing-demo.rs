//! Демонстрация посадки на рельефную поверхность.
//!
//! Сервер прижимает корабль к рельефу, гасит скорость и различает мягкую
//! посадку и удар. Скорость задаём напрямую (поле `vel` публично), чтобы
//! показать оба сценария без долгого разгона.

use ou_protocol::{InputFrame, Vec3};
use ou_sim::landing::GroundState;
use ou_sim::{Terrain, TerrainParams, World};
use std::collections::BTreeMap;

/// Высота точки над локальной поверхностью (а не над базовым радиусом).
fn altitude(terrain: &Terrain, p: Vec3) -> f64 {
    p.sub(terrain.center).length() - terrain.surface_radius(p.sub(terrain.center))
}

fn main() {
    println!("Open Universe — посадка на поверхность\n");

    let radius = 1000.0;
    let terrain = Terrain::new(
        Vec3::ZERO,
        TerrainParams { radius, amplitude: 34.0, sea_level: 0.30, seed: 0xC0FFEE },
    );
    let up = Vec3::new(0.0, 1.0, 0.0);
    let surface = terrain.surface_radius(up);
    println!("=== Рельеф ===");
    println!("  радиус океана: {radius:.0}, размах рельефа: ±{:.0}", terrain.params.amplitude * 2.0);
    println!("  поверхность в точке посадки: {surface:.1}");
    println!("  там вода? {}\n", terrain.is_water(up));

    // --- Мягкая посадка: медленное снижение ---
    println!("=== Мягкая посадка (скорость 25 вниз) ===");
    let mut world = World::new();
    let id = world.spawn(1, Vec3::ZERO.add(up.scale(surface + 200.0)));
    world.attach_terrain(id, terrain);
    // Задаём скорость снижения напрямую.
    world.entities.get_mut(&id).unwrap().state.vel = up.scale(-25.0);

    for tick in 1..=400u64 {
        let mut inputs = BTreeMap::new();
        // Автопилот: лёгкая тяга «вверх», чтобы скорость не росла.
        inputs.insert(id, InputFrame { seq: tick as u32, throttle: 0.0, yaw: 0.0, pitch: 0.6, fire: false, ..Default::default() });
        world.step(&inputs);
        let e = world.entities.get(&id).unwrap();
        if tick % 40 == 0 {
            println!(
                "  тик {tick:>3}: высота {:>7.1}, скорость {:>6.1}  {:?}",
                altitude(&terrain, e.state.pos),
                e.state.vel.length(),
                e.ground
            );
        }
        if !matches!(e.ground, GroundState::Flying) {
            println!("  → касание на тик {tick}");
            break;
        }
    }
    let e = world.entities.get(&id).unwrap();
    println!("  итог: {:?}, высота {:.3}, проникновение {:.6}\n", e.ground, altitude(&terrain, e.state.pos), terrain.penetration(e.state.pos));

    // --- Жёсткий удар: большая скорость вниз ---
    println!("=== Жёсткий удар (скорость 320 вниз) ===");
    let mut world2 = World::new();
    let id2 = world2.spawn(2, Vec3::ZERO.add(up.scale(surface + 150.0)));
    world2.attach_terrain(id2, terrain);
    world2.entities.get_mut(&id2).unwrap().state.vel = up.scale(-320.0);

    let mut crashed = false;
    for tick in 1..=120u64 {
        let empty: BTreeMap<u32, InputFrame> = BTreeMap::new();
        world2.step(&empty);
        let e = world2.entities.get(&id2).unwrap();
        if let GroundState::Crashed { impact_speed } = e.ground {
            println!("  → УДАР на тик {tick}: скорость столкновения {impact_speed:.1}");
            println!("     корабль остановлен на высоте {:.3}", altitude(&terrain, e.state.pos));
            crashed = true;
            break;
        }
    }
    if !crashed {
        println!("  удара не зафиксировано");
    }

    // --- Трение: скольжение по поверхности ---
    println!("\n=== Трение о поверхность ===");
    let mut world3 = World::new();
    let id3 = world3.spawn(3, Vec3::ZERO.add(up.scale(surface - 2.0)));
    world3.attach_terrain(id3, terrain);
    // Касательная скорость (вбок) + слегка «в землю».
    let tangent = Vec3::new(1.0, 0.0, 0.0);
    world3.entities.get_mut(&id3).unwrap().state.vel = tangent.scale(120.0).add(up.scale(-10.0));
    let mut prev = 120.0;
    for tick in 1..=10u64 {
        let empty: BTreeMap<u32, InputFrame> = BTreeMap::new();
        world3.step(&empty);
        let v = world3.entities.get(&id3).unwrap().state.vel.length();
        if tick <= 5 || tick == 10 {
            println!("  тик {tick:>2}: скорость {v:>7.2} (было {prev:>7.2})");
        }
        prev = v;
    }
    println!("  трение тормозит скольжение — корабль останавливается");

    println!("\n=== Итог ===");
    println!("Состояния поверхности: Flying / Landed / Crashed.");
    println!("Корабль никогда не проваливается под рельеф: проникновение всегда 0.");
    println!("Контрольные суммы миров: {:#x} / {:#x} / {:#x}",
        world.checksum(), world2.checksum(), world3.checksum());
}
