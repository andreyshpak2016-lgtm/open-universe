//! Демонстрация экономики: рынок, стакан, сделки и производственная цепочка.
//!
//! Показывает, как игроки торгуют между собой, как формируется цена и как
//! сырьё превращается в корабль.

use ou_economy::{
    Inventory, ItemRegistry, MatchingEngine, Market, Production, Side, Trade,
};
use std::collections::BTreeMap;

fn main() {
    println!("Open Universe — экономика и рынок\n");

    let reg = ItemRegistry::default_universe();
    let find = |n: &str| -> u32 { (1..=reg.len() as u32).find(|i| reg.name(*i) == n).unwrap() };
    let ore = find("руда");
    let metal = find("металл");
    let ship = find("корабль");

    // Одна звёздная система, три игрока-добытчика, один промышленник.
    let sys = 1;
    let mut inv = Inventory::new();
    for (owner, ore_qty) in [(1u32, 120u64), (2, 80), (3, 150)] {
        inv.add(owner, sys, ore, ore_qty);
    }
    println!("=== Стартовые запасы руды ===");
    for o in [1u32, 2, 3] {
        println!("  игрок {o}: {} руды", inv.amount(o, sys, ore));
    }

    let mut eng = MatchingEngine::new(Market::new(), inv);

    // --- Игроки выставляют продажи по разным ценам ---
    println!("\n=== Ордера на продажу руды ===");
    for (owner, price, qty) in [(1u32, 30u64, 40u64), (2, 25, 60), (3, 28, 100)] {
        let r = eng.place_sell(sys, ore, owner, price, qty, 1).unwrap();
        println!("  игрок {owner}: продаёт {qty} по {price} (id {})", r.order_id);
    }

    let (bids, asks) = eng.market.depth(sys, ore, 5);
    println!("\nСтакан руды:");
    println!("  продажи (от лучшей): {:?}", asks.iter().map(|l| (l.price, l.qty)).collect::<Vec<_>>());
    println!("  покупки (от лучшей): {:?}", bids.iter().map(|l| (l.price, l.qty)).collect::<Vec<_>>());

    // --- Покупатель берёт 100 руды по 30 ---
    println!("\n=== Покупатель (игрок 9) берёт 100 руды по лимиту 30 ===");
    let r = eng.place_buy(sys, ore, 9, 30, 100, 2).unwrap();
    println!("Исполнено: {} из 100", r.filled);
    let mut spent: u64 = 0;
    for t in &r.trades {
        spent += t.total();
        println!(
            "  сделка: продавец {} → покупатель {}, {} шт по {} (итого {})",
            t.seller, t.buyer, t.qty, t.price, t.total()
        );
    }
    println!("Покупатель заплатил всего: {spent}");
    println!("Руда у покупателя: {}", eng.inventory.amount(9, sys, ore));

    let (_, asks2) = eng.market.depth(sys, ore, 5);
    println!("Остаток в стакане продаж: {:?}", asks2.iter().map(|l| (l.price, l.qty)).collect::<Vec<_>>());

    // --- Итоговая цена (VWAP) ---
    println!("\n=== Средневзвешенная цена сделок (VWAP) ===");
    let all: Vec<&Trade> = eng.trades.iter().collect();
    let total_qty: u64 = all.iter().map(|t| t.qty).sum();
    let total_val: u64 = all.iter().map(|t| t.total()).sum();
    if total_qty > 0 {
        println!("  объём: {total_qty} шт, оборот: {total_val} кр, VWAP = {}", total_val / total_qty);
    }

    // --- Производство: руда → металл → ... ---
    println!("\n=== Производственная цепочка ===");
    let prod = Production::default_chain(&reg);
    // Дадим игроку 9 его руду и часть кристаллов, и гоним цепочку.
    eng.inventory.add(9, sys, find("кристалл"), 100);
    let mut crafted_total = 0u32;
    for _pass in 0..30 {
        for rid in 1..=prod.len() as u32 {
            while prod.can_craft(rid, &eng.inventory, 9, sys) {
                prod.craft(rid, &mut eng.inventory, 9, sys).unwrap();
                crafted_total += 1;
            }
        }
    }
    println!("Изготовлено операций: {crafted_total}");
    for i in 1..=reg.len() as u32 {
        let a = eng.inventory.amount(9, sys, i);
        if a > 0 {
            println!("  {:<12} = {}", reg.name(i), a);
        }
    }

    println!("\n=== Как это связано со схемой данных сайта ===");
    let mapping: BTreeMap<&str, &str> = BTreeMap::from([
        ("inventory_items", "Inventory — что и у кого лежит"),
        ("market_orders", "Market — ордера в стакане"),
        ("transactions", "MatchingEngine.trades — журнал сделок"),
        ("blueprints", "Production — рецепты конструкций"),
    ]);
    for (table, here) in &mapping {
        println!("  {table:<16} → {here}");
    }

    println!("\nВсего сделок в журнале: {}", eng.trades.len());
    // Проверка сохранения материи: руда ушла в переработку, поэтому проверяем
    // сумму «сырьё + продукты» в инвентаре покупателя плюс остаток в стакане.
    let created: u64 = (1..=reg.len() as u32)
        .map(|i| eng.inventory.amount(9, sys, i))
        .sum();
    let in_book: u64 = eng.market.depth(sys, ore, 5).1.iter().map(|l| l.qty).sum();
    println!(
        "У покупателя предметов: {created}, в стакане продаж осталось: {in_book}"
    );
    let _ = (metal, ship, Side::Buy);
}
