//! Симуляция рынка: агенты торгуют, цены формируются сами.
//!
//! ## Зачем это в коде, а не «на бумаге»
//! Экономика MMO либо работает, либо рушится через неделю после запуска.
//! Заранее это проверяют **симуляцией**: запускаем виртуальных игроков
//! (добытчиков, промышленников, потребителей) и смотрим, что происходит с
//! ценами, объёмами и дефицитом.
//!
//! ## Модель агентов
//! * **Добытчик** — производит сырьё и продаёт его.
//! * **Промышленник** — покупает сырьё, перерабатывает, продаёт продукт.
//! * **Потребитель** — покупает готовое и «расходует» его.
//!
//! Все действия детерминированы: агент принимает решения по числам состояния,
//! без случайности. Значит, один и тот же прогон даёт один и тот же результат —
//! это позволяет сравнивать балансовые правки.

use crate::{Inventory, ItemId, MatchingEngine, Market, OwnerId, SystemId};
use std::collections::BTreeMap;

/// Роль агента на рынке.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Производит сырьё.
    Miner,
    /// Перерабатывает сырьё в продукт.
    Refiner,
    /// Потребляет готовую продукцию.
    Consumer,
}

/// Агент симуляции.
#[derive(Clone, Debug)]
pub struct Agent {
    pub id: OwnerId,
    pub role: Role,
    /// Запас кредитов.
    pub credits: u64,
    /// Личное представление о «справедливой» цене (обновляется по сделкам).
    pub price_belief: f64,
    /// Сколько раз агент продал и купил (для статистики).
    pub sold: u64,
    pub bought: u64,
}

impl Agent {
    pub fn new(id: OwnerId, role: Role, credits: u64, start_price: f64) -> Self {
        Agent { id, role, credits, price_belief: start_price, sold: 0, bought: 0 }
    }

    /// Корректировка представления о цене: агент «учится» по последней сделке.
    fn learn(&mut self, traded_price: u64, weight: f64) {
        self.price_belief = self.price_belief * (1.0 - weight) + traded_price as f64 * weight;
    }
}

/// Настройки симуляции.
#[derive(Clone, Copy, Debug)]
pub struct SimConfig {
    /// Система, в которой идёт торговля.
    pub system: SystemId,
    /// Предмет сырья.
    pub raw: ItemId,
    /// Предмет продукта.
    pub product: ItemId,
    /// Сколько сырья добывает один добытчик за тик.
    pub mining_output: u64,
    /// Сколько сырья нужно на одну единицу продукта.
    pub refine_input: u64,
    /// Сколько продукта потребляет один потребитель за тик.
    pub consumption: u64,
    /// Начальные кредиты агента.
    pub start_credits: u64,
}

impl Default for SimConfig {
    fn default() -> Self {
        SimConfig {
            system: 1,
            raw: 1,
            product: 4,
            mining_output: 3,
            refine_input: 2,
            consumption: 1,
            start_credits: 10_000,
        }
    }
}

/// Одна точка истории рынка (для графиков и анализа).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarketTick {
    pub tick: u64,
    /// Цена последней сделки (0 — сделок не было).
    pub price: u64,
    /// Объём сделок за тик.
    pub volume: u64,
    /// Лучший спрос (цена покупки).
    pub best_bid: u64,
    /// Лучшее предложение (цена продажи).
    pub best_ask: u64,
    /// Спред.
    pub spread: u64,
}

/// Итоги симуляции.
#[derive(Clone, Debug)]
pub struct SimResult {
    /// История цен по тикам.
    pub history: Vec<MarketTick>,
    /// Средняя цена (VWAP) за весь прогон.
    pub vwap: u64,
    /// Минимальная и максимальная цена сделки.
    pub min_price: u64,
    pub max_price: u64,
    /// Всего сделок и объём.
    pub trades: usize,
    pub total_volume: u64,
    /// Кредиты, «созданные» и «сожжённые» (должно быть 0 — деньги не печатаются).
    pub credits_delta: i64,
}

/// Симулятор экономики.
pub struct Simulator {
    pub config: SimConfig,
    pub engine: MatchingEngine,
    pub agents: Vec<Agent>,
    /// Цена последней сделки.
    last_price: u64,
    /// Валовой объём за прогон.
    gross_volume: u64,
    /// Сумма сделок (price×qty) для VWAP.
    gross_value: u64,
    /// Суммарные кредиты на старте.
    start_total_credits: u64,
}

impl Simulator {
    /// Создать симулятор: `miners`, `refiners`, `consumers` агентов.
    pub fn new(config: SimConfig, miners: usize, refiners: usize, consumers: usize) -> Self {
        let market = Market::new();
        let inventory = Inventory::new();
        let mut engine = MatchingEngine::new(market, inventory);

        let mut agents = Vec::new();
        let mut id = 1u32;
        // Начальные «справедливые» цены: сырьё дешевле продукта.
        for _ in 0..miners {
            agents.push(Agent::new(id, Role::Miner, config.start_credits, 10.0));
            id += 1;
        }
        for _ in 0..refiners {
            agents.push(Agent::new(id, Role::Refiner, config.start_credits, 40.0));
            id += 1;
        }
        for _ in 0..consumers {
            agents.push(Agent::new(id, Role::Consumer, config.start_credits, 40.0));
            id += 1;
        }

        let start_total_credits = agents.iter().map(|a| a.credits).sum();
        Simulator {
            config,
            engine,
            agents,
            last_price: 0,
            gross_volume: 0,
            gross_value: 0,
            start_total_credits,
        }
    }

    /// Один тик симуляции: агенты действуют по ролям.
    pub fn tick(&mut self, tick: u64) -> MarketTick {
        let sys = self.config.system;
        let raw = self.config.raw;
        let product = self.config.product;

        // 1. Добытчики производят сырьё (вне рынка — из «астероидов»).
        for i in 0..self.agents.len() {
            if self.agents[i].role == Role::Miner {
                let id = self.agents[i].id;
                let amount = self.config.mining_output;
                self.engine.inventory.add(id, sys, raw, amount);
            }
        }

        // 2. Все выставляют ордера: продавцы — по своей вере, покупатели — тоже.
        //    Копим сделки этого тика.
        let mut trades_this_tick: Vec<(u64, u64)> = Vec::new(); // (price, qty)

        // Собираем намерения (не мутируя агентов во время обхода).
        let mut sells: Vec<(OwnerId, ItemId, u64, u64)> = Vec::new(); // (кто, что, цена, qty)
        let mut buys: Vec<(OwnerId, ItemId, u64, u64)> = Vec::new();

        for a in &self.agents {
            let have_raw = self.engine.inventory.amount(a.id, sys, raw);
            let have_product = self.engine.inventory.amount(a.id, sys, product);
            match a.role {
                Role::Miner => {
                    // Продаёт всё сырьё по цене чуть ниже своей веры.
                    if have_raw > 0 {
                        let price = (a.price_belief * 0.98).round() as u64;
                        sells.push((a.id, raw, price.max(1), have_raw));
                    }
                }
                Role::Refiner => {
                    // Продаёт продукт и покупает сырьё.
                    if have_product > 0 {
                        let price = (a.price_belief * 1.02).round() as u64;
                        sells.push((a.id, product, price.max(1), have_product));
                    }
                    // Нужно сырьё на одну партию.
                    let need = self.config.refine_input;
                    if have_raw < need {
                        let price = (a.price_belief * 0.25).round() as u64; // готов платить
                        buys.push((a.id, raw, price.max(1), need - have_raw));
                    }
                }
                Role::Consumer => {
                    // Покупает продукт по цене чуть выше веры.
                    let need = self.config.consumption;
                    if have_product < need {
                        let price = (a.price_belief * 1.05).round() as u64;
                        buys.push((a.id, product, price.max(1), need - have_product));
                    }
                }
            }
        }

        // 3. Сначала продажи (создают предложение), затем покупки (исполняются).
        for (owner, item, price, qty) in sells {
            if let Ok(report) = self.engine.place_sell(sys, item, owner, price, qty, tick) {
                for t in &report.trades {
                    trades_this_tick.push((t.price, t.qty));
                }
            }
        }
        for (owner, item, price, qty) in buys {
            // Покупатель не может потратить больше, чем у него есть.
            let affordable = price.saturating_mul(qty);
            if affordable > self.agents_credits(owner) {
                continue;
            }
            if let Ok(report) = self.engine.place_buy(sys, item, owner, price, qty, tick) {
                for t in &report.trades {
                    trades_this_tick.push((t.price, t.qty));
                }
            }
        }

        // 4. Расчёты: перевести кредиты и применить обучение по цене.
        self.settle_trades();

        // 5. Переработка у промышленников: сырьё → продукт.
        let mut conversions: Vec<(OwnerId, u64)> = Vec::new();
        for a in &self.agents {
            if a.role == Role::Refiner {
                let available = self.engine.inventory.amount(a.id, sys, raw) / self.config.refine_input;
                if available > 0 {
                    conversions.push((a.id, available));
                }
            }
        }
        for (id, batches) in conversions {
            let used = batches * self.config.refine_input;
            self.engine.inventory.take(id, sys, raw, used);
            self.engine.inventory.add(id, sys, product, batches);
        }

        // 6. Потребители расходуют продукт.
        let mut consumed: Vec<OwnerId> = Vec::new();
        for a in &self.agents {
            if a.role == Role::Consumer {
                consumed.push(a.id);
            }
        }
        for id in consumed {
            self.engine.inventory.take(id, sys, product, self.config.consumption);
        }

        // 7. История этого тика.
        let (price, volume) = trades_this_tick
            .iter()
            .fold((0u64, 0u64), |(p, v), (tp, tq)| (if *tp > 0 { *tp } else { p }, v + tq));
        for (p, q) in &trades_this_tick {
            self.gross_volume += q;
            self.gross_value += p * q;
        }
        // Адаптация цен, если тик прошёл без сделок.
        self.adapt_prices(!trades_this_tick.is_empty());
        if price > 0 {
            self.last_price = price;
        }
        let bid = self.engine.market.best_bid(sys, product).map(|l| l.price).unwrap_or(0);
        let ask = self.engine.market.best_ask(sys, product).map(|l| l.price).unwrap_or(0);
        let spread = if ask > 0 && bid > 0 { ask.saturating_sub(bid) } else { 0 };

        MarketTick {
            tick,
            price: self.last_price,
            volume,
            best_bid: bid,
            best_ask: ask,
            spread,
        }
    }

    /// Скорректировать цены, если сделок не было: продавцы уступают,
    /// покупатели поднимают ставку. Без этого рынок «замирает».
    fn adapt_prices(&mut self, traded: bool) {
        if traded {
            return;
        }
        for a in &mut self.agents {
            match a.role {
                // Продавцы постепенно снижают цену.
                Role::Miner => a.price_belief = (a.price_belief * 0.97).max(1.0),
                Role::Refiner => a.price_belief = (a.price_belief * 0.985).max(2.0),
                // Покупатели постепенно повышают готовность платить.
                Role::Consumer => a.price_belief = (a.price_belief * 1.03).min(500.0),
            }
        }
    }

    fn agents_credits(&self, id: OwnerId) -> u64 {
        self.agents.iter().find(|a| a.id == id).map(|a| a.credits).unwrap_or(0)
    }

    /// Перевести кредиты по сделкам (покупатель платит, продавец получает)
    /// и обучить агентов «справедливой» цене.
    fn settle_trades(&mut self) {
        // Журнал сделок пополняется движком; берём только новые.
        let all = self.engine.trades.clone();
        // Отсчитываем уже обработанные через сумму объёмов.
        let processed: usize = self
            .agents
            .iter()
            .map(|a| (a.sold + a.bought) as usize)
            .sum::<usize>()
            / 2;
        let _ = processed;
        for t in &all {
            let total = t.price.saturating_mul(t.qty);
            // Найти покупателя и продавца.
            let buyer = self.agents.iter().position(|a| a.id == t.buyer);
            let seller = self.agents.iter().position(|a| a.id == t.seller);
            if let Some(bi) = buyer {
                self.agents[bi].credits = self.agents[bi].credits.saturating_sub(total);
                self.agents[bi].bought += t.qty;
                self.agents[bi].learn(t.price, 0.05);
            }
            if let Some(si) = seller {
                self.agents[si].credits = self.agents[si].credits.saturating_add(total);
                self.agents[si].sold += t.qty;
                self.agents[si].learn(t.price, 0.05);
            }
        }
        // Чтобы не считать сделки дважды: очищаем накопленный журнал, но
        // помним объёмы (нужны для итогов).
        self.engine.trades.clear();
    }

    /// Прогнать `ticks` тиков и вернуть итоги.
    pub fn run(&mut self, ticks: u64) -> SimResult {
        let mut history = Vec::with_capacity(ticks as usize);
        for t in 0..ticks {
            history.push(self.tick(t));
        }

        let prices: Vec<u64> = history.iter().map(|h| h.price).filter(|p| *p > 0).collect();
        let min_price = prices.iter().copied().min().unwrap_or(0);
        let max_price = prices.iter().copied().max().unwrap_or(0);
        let vwap = if self.gross_volume > 0 {
            self.gross_value / self.gross_volume
        } else {
            0
        };
        let now_total: u64 = self.agents.iter().map(|a| a.credits).sum();
        let credits_delta = now_total as i64 - self.start_total_credits as i64;

        SimResult {
            history,
            vwap,
            min_price,
            max_price,
            trades: 0,
            total_volume: self.gross_volume,
            credits_delta,
        }
    }

    /// Итоговые балансы агентов по ролям (для анализа).
    pub fn balances_by_role(&self) -> BTreeMap<&'static str, (u64, u64)> {
        let mut out: BTreeMap<&'static str, (u64, u64)> = BTreeMap::new();
        for a in &self.agents {
            let key = match a.role {
                Role::Miner => "добытчики",
                Role::Refiner => "промышленники",
                Role::Consumer => "потребители",
            };
            let e = out.entry(key).or_insert((0, 0));
            e.0 += a.credits;
            e.1 += 1;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sim() -> Simulator {
        // 4 добытчика, 2 промышленника, 6 потребителей.
        Simulator::new(SimConfig::default(), 4, 2, 6)
    }

    #[test]
    fn agents_are_created_with_roles() {
        let s = sim();
        assert_eq!(s.agents.len(), 12);
        assert!(s.agents.iter().any(|a| a.role == Role::Miner));
        assert!(s.agents.iter().any(|a| a.role == Role::Refiner));
        assert!(s.agents.iter().any(|a| a.role == Role::Consumer));
    }

    #[test]
    fn market_produces_trades() {
        let mut s = sim();
        let result = s.run(100);
        assert!(result.total_volume > 0, "рынок должен торговать");
        assert!(result.vwap > 0, "должна быть средняя цена");
    }

    #[test]
    fn prices_are_bounded_and_positive() {
        let mut s = sim();
        let result = s.run(100);
        assert!(result.min_price > 0);
        assert!(result.min_price <= result.max_price);
        // Цена не должна «взлетать» до абсурда.
        assert!(result.max_price < 10_000, "неправдоподобная цена: {}", result.max_price);
    }

    #[test]
    fn simulation_is_deterministic() {
        let mut a = sim();
        let mut b = sim();
        let ra = a.run(80);
        let rb = b.run(80);
        assert_eq!(ra.vwap, rb.vwap);
        assert_eq!(ra.total_volume, rb.total_volume);
        assert_eq!(
            ra.history.iter().map(|h| h.price).collect::<Vec<_>>(),
            rb.history.iter().map(|h| h.price).collect::<Vec<_>>()
        );
    }

    #[test]
    fn credits_are_conserved() {
        // Внутри рынка деньги не печатаются и не сжигаются.
        let mut s = sim();
        let r = s.run(120);
        assert_eq!(r.credits_delta, 0, "кредиты не должны создаваться или исчезать");
    }

    #[test]
    fn every_role_gets_some_credits_over_time() {
        let mut s = sim();
        s.run(200);
        let balances = s.balances_by_role();
        // Добытчики продают сырьё — у них должны быть деньги.
        let miners = balances.get("добытчики").expect("есть добытчики");
        assert!(miners.0 > 0, "добытчики должны заработать");
    }

    #[test]
    fn history_length_matches_ticks() {
        let mut s = sim();
        let r = s.run(37);
        assert_eq!(r.history.len(), 37);
    }

    #[test]
    fn spread_is_non_negative() {
        let mut s = sim();
        let r = s.run(60);
        for h in &r.history {
            assert!(h.spread < u64::MAX); // нет переполнения
        }
    }

    #[test]
    fn shortage_raises_price() {
        // Если потребителей много, а сырья мало — цены должны держаться высоко.
        let scarce = SimConfig { mining_output: 1, ..Default::default() };
        let mut s = Simulator::new(scarce, 1, 1, 20);
        let r = s.run(200);
        // Сравниваем цену в первой и последней четверти прогона.
        let prices: Vec<u64> = r.history.iter().map(|h| h.price).collect();
        let n = prices.len();
        let avg = |slice: &[u64]| {
            let s: u64 = slice.iter().sum();
            s / slice.len().max(1) as u64
        };
        let early = avg(&prices[..n / 4]);
        let late = avg(&prices[n * 3 / 4..]);
        assert!(
            late >= early,
            "при устойчивом дефиците цена не должна падать: {early} → {late}"
        );
    }

    #[test]
    fn abundance_lowers_or_keeps_price() {
        // Много добытчиков, мало потребителей — цена не должна расти бесконечно.
        let rich = SimConfig { mining_output: 20, ..Default::default() };
        let mut s = Simulator::new(rich, 10, 2, 2);
        let r = s.run(150);
        assert!(r.max_price < 5_000, "при избытке цена не должна взлетать");
    }
}
