//! Демонстрация симуляции рынка: цены, объёмы, дефицит и избыток.

use ou_economy::{SimConfig, Simulator};

fn sparkline(values: &[u64]) -> String {
    if values.is_empty() {
        return String::new();
    }
    let blocks = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let max = *values.iter().max().unwrap_or(&1);
    let min = *values.iter().min().unwrap_or(&0);
    let span = (max - min).max(1);
    values
        .iter()
        .map(|v| blocks[(((*v - min) as f64 / span as f64) * 7.0).round() as usize])
        .collect()
}

fn run_case(title: &str, config: SimConfig, miners: usize, refiners: usize, consumers: usize) {
    let mut sim = Simulator::new(config, miners, refiners, consumers);
    let result = sim.run(200);

    println!("=== {title} ===");
    println!(
        "  агенты: {} добытчиков, {} промышленников, {} потребителей",
        miners, refiners, consumers
    );
    println!("  сделок всего: объём {}", result.total_volume);
    println!(
        "  цена: min {}, max {}, VWAP {}",
        result.min_price, result.max_price, result.vwap
    );
    let prices: Vec<u64> = result.history.iter().map(|h| h.price).collect();
    println!("  динамика цены:   {}", sparkline(&prices));
    let volumes: Vec<u64> = result.history.iter().map(|h| h.volume).collect();
    println!("  динамика объёма: {}", sparkline(&volumes));
    // Активность: доля тиков со сделками.
    let active = volumes.iter().filter(|v| **v > 0).count();
    println!(
        "  активных тиков: {active} из {} ({:.0}%)",
        volumes.len(),
        active as f64 / volumes.len().max(1) as f64 * 100.0
    );
    println!("  проверка денег: создано/сожжено кредитов = {}", result.credits_delta);

    let balances = sim.balances_by_role();
    for (role, (credits, count)) in &balances {
        if *count > 0 {
            println!("   {role:<15} всего {credits:>8} кр (на агента {})", credits / count);
        }
    }
    println!();
}

fn main() {
    println!("Open Universe — симуляция рынка (проверка экономической модели)\n");

    // 1. Сбалансированная модель.
    run_case("Сбалансированная экономика", SimConfig::default(), 4, 2, 6);

    // 2. Дефицит сырья: мало добытчиков, много потребителей.
    run_case(
        "Дефицит: мало сырья, много потребителей",
        SimConfig { mining_output: 1, ..Default::default() },
        1,
        1,
        12,
    );

    // 3. Избыток: много добытчиков, мало потребителей.
    run_case(
        "Избыток: много сырья, мало потребителей",
        SimConfig { mining_output: 20, ..Default::default() },
        10,
        2,
        2,
    );

    // 4. Дорогая переработка.
    run_case(
        "Дорогая переработка (много сырья на продукт)",
        SimConfig { refine_input: 8, ..Default::default() },
        6,
        2,
        6,
    );

    println!("=== Выводы модели ===");
    println!("• Деньги внутри рынка сохраняются: они переходят между игроками, не печатаются.");
    println!("• Спрос и предложение двигают цену: дефицит поднимает, избыток сдерживает.");
    println!("• Себестоимость переработки напрямую влияет на цену продукта.");
    println!("• Симуляция детерминирована — правки баланса сравнимы между прогонами.");
}
