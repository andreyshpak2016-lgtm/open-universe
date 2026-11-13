//! Эталон рельефа для кросс-проверки сервер↔клиент.
//!
//! Пишет высоты в наборе направлений в файл (UTF-8). C++-стенд
//! (`client/testdata/test_terrain.cpp`) считает те же направления общей формулой
//! (`OUTerrain.h`) и сверяет с этим эталоном. Так гарантируется, что картинка на
//! клиенте совпадает с физикой.
//!
//! Запуск: cargo run -p ou-sim --bin ou-terrain-ref -- <файл>
//!
//! ВАЖНО: пишем файл сами, а не через перенаправление PowerShell — иначе вывод
//! станет UTF-16 и C++-стенд не сможет его разобрать.

use ou_sim::{home_planet, Vec3};
use std::io::Write;

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "terrain_ref.txt".to_string());
    let t = home_planet();

    // Направления: полюсы, оси и произвольные — покрывают все ветви
    // октаэдрического отображения (включая нижнюю полусферу).
    let dirs: Vec<Vec3> = vec![
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, -1.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(-1.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(0.0, 0.0, -1.0),
        Vec3::new(1.0, 1.0, 1.0),
        Vec3::new(-1.0, 0.7, 0.3),
        Vec3::new(0.3, -0.5, 0.8),
        Vec3::new(-0.25, -0.9, 0.35),
        Vec3::new(0.9, 0.1, -0.4),
        Vec3::new(0.5, 0.5, -0.7),
        Vec3::new(-0.83, -0.21, 0.51),
        Vec3::new(0.14, 0.93, -0.34),
        Vec3::new(-0.62, 0.44, -0.65),
        Vec3::new(0.77, -0.63, 0.09),
    ];

    let mut out = String::new();
    out.push_str(&format!("version {}\n", ou_sim::TERRAIN_VERSION));
    out.push_str(&format!("radius {:.1}\n", t.params.radius));
    out.push_str(&format!("amplitude {:.1}\n", t.params.amplitude));
    out.push_str(&format!("sea_level {:.6}\n", t.params.sea_level));
    out.push_str(&format!("seed {}\n", t.params.seed));
    // Точка появления (самое ровное место): клиент обязан выбрать ту же.
    let sp = ou_sim::home_spawn_direction();
    out.push_str(&format!("spawn {:.17} {:.17} {:.17}\n", sp.x, sp.y, sp.z));
    // Воронка копания: клиент обязан посчитать ту же изменённую высоту.
    {
        let mut td = t;
        let dent = ou_sim::Dent { x: sp.x, y: sp.y, z: sp.z, cos_r: 0.9999, depth: 40.0 };
        ou_sim::Dent::place(&mut td.dents, &mut td.dent_count, 0, dent);
        let dug = td.height(sp);
        out.push_str(&format!("dent {:.17} {:.17} {:.17} {:.17} {:.17} {:.17}\n",
            dent.x, dent.y, dent.z, dent.cos_r, dent.depth, dug));
    }
    for d in &dirs {
        // Оба уровня детализации: `0` — глобальный меш, `1` — физика/локальный
        // участок. Клиент обязан совпасть с каждым бит-в-бит.
        let h0 = t.height_detail(*d, 0);
        let h1 = t.height_detail(*d, 1);
        let b = t.biome(*d).code();
        out.push_str(&format!(
            "height {:.17} {:.17} {:.17} {:.17} {:.17} {b}\n",
            d.x, d.y, d.z, h0, h1
        ));
    }

    match std::fs::File::create(&path) {
        Ok(mut f) => {
            if f.write_all(out.as_bytes()).is_err() {
                eprintln!("не записать эталон: {path}");
                std::process::exit(1);
            }
            println!("эталон записан: {path} ({} направлений)", dirs.len());
        }
        Err(e) => {
            eprintln!("не создать файл {path}: {e}");
            std::process::exit(1);
        }
    }
}
