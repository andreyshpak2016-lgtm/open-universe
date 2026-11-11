//! Демонстрация воксельного редактора: строим корабль по шагам.
//!
//! Показывает все три операции (поставить, удалить, покрасить), проверку
//! целостности и реакцию на отвалившиеся детали.

use ou_build::{ship_blueprint, Color, VoxelGrid};

/// ASCII-проекция конструкции (вид сверху), чтобы «увидеть» результат.
fn top_view(g: &VoxelGrid) -> String {
    let (w, _h, _d) = g.extent();
    if w == 0 {
        return "(пусто)".into();
    }
    let cells: Vec<(i32, i32, i32)> = g.iter().map(|(c, _)| c).collect();
    let min_x = cells.iter().map(|c| c.0).min().unwrap();
    let max_x = cells.iter().map(|c| c.0).max().unwrap();
    let min_z = cells.iter().map(|c| c.2).min().unwrap();
    let max_z = cells.iter().map(|c| c.2).max().unwrap();

    let occupied: std::collections::BTreeSet<(i32, i32)> =
        cells.iter().map(|c| (c.0, c.2)).collect();

    let mut out = String::new();
    for z in (min_z..=max_z).rev() {
        for x in min_x..=max_x {
            out.push(if occupied.contains(&(x, z)) { '■' } else { '·' });
        }
        out.push('\n');
    }
    out
}

fn main() {
    println!("Open Universe — воксельный редактор конструкций\n");

    // --- 1. Ядро и первая деталь ---
    let mut g = VoxelGrid::new();
    println!("=== Строительство с нуля ===");
    println!("Старт: {} воксель (ядро в 0,0,0)", g.len());

    // Ставим вдоль +Z.
    for z in 1..=6 {
        let r = g.place((0, 0, z), if z % 2 == 0 { Color::HULL } else { Color::DARK });
        if r.is_err() {
            println!("  ошибка на z={z}: {:?}", r.err());
        }
    }
    println!("После установки 6 деталей: {} вокселей", g.len());

    // --- 2. Проверка: нельзя ставить в пустоте ---
    println!("\n=== Пробуем поставить в отрыве от корпуса ===");
    match g.place((10, 0, 0), Color::RED) {
        Ok(()) => println!("  ОШИБКА: оторванный воксель разрешён!"),
        Err(e) => println!("  запрещено: {e}"),
    }

    // --- 3. Покраска ---
    println!("\n=== Красим деталь в акцентный цвет ===");
    g.paint((0, 0, 3), Color::ACCENT).unwrap();
    let (r, gg, b) = g.get((0, 0, 3)).unwrap().to_f64();
    println!("  цвет в (0,0,3): rgb({r:.2}, {gg:.2}, {b:.2})");

    // --- 4. Крылья и кабина ---
    println!("\n=== Добавляем крылья и кабину ===");
    for x in [-1, 1] {
        for z in 2..=4 {
            let _ = g.place((x, 0, z), Color::ACCENT);
        }
    }
    let _ = g.place((0, 1, 3), Color::GLASS);
    println!("Вокселей: {}, габариты {:?}", g.len(), g.extent());
    println!("\nВид сверху:");
    print!("{}", top_view(&g));

    // --- 5. Разрушение: снимаем середину ---
    println!("=== Ломаем: снимаем воксель в середине ===");
    g.remove((0, 0, 3)).unwrap();
    println!("Вокселей: {}", g.len());
    match g.validate() {
        Ok(()) => println!("  целостность: ОК"),
        Err(e) => println!("  целостность нарушена: {e}"),
    }
    let pruned = g.prune_disconnected();
    if pruned > 0 {
        println!("  снято оторванных деталей: {pruned}");
    }
    println!("После уборки вокселей: {}", g.len());
    println!("Целостность: {:?}", g.validate().is_ok());

    // --- 6. Лимиты ---
    println!("\n=== Лимиты редактора ===");
    let mut small = VoxelGrid::with_limits(ou_build::BuildLimits {
        max_voxels: 5,
        ..Default::default()
    });
    let mut placed = 1;
    for z in 1..=10 {
        if small.place((0, 0, z), Color::HULL).is_ok() {
            placed += 1;
        } else {
            break;
        }
    }
    println!("  лимит 5 вокселей: удалось поставить {placed}");

    // --- 7. Шаблон корабля ---
    println!("\n=== Готовый шаблон корабля (blueprint) ===");
    let bp = ship_blueprint();
    println!("  вокселей: {}, габариты {:?}", bp.len(), bp.extent());
    println!("  масса (2 ед/воксель): {:.0}", bp.mass(2.0));
    println!("  связность: {}", if bp.validate().is_ok() { "ОК" } else { "нарушена" });
    println!("\nВид сверху:");
    print!("{}", top_view(&bp));

    // --- 8. Детерминизм ---
    println!("=== Детерминизм ===");
    let a = ship_blueprint().checksum();
    let b = ship_blueprint().checksum();
    println!("  контрольная сумма: {a:#x}");
    println!("  повтор даёт то же: {}", a == b);
}
