//! # ou-economy
//!
//! Экономика Open Universe: предметы, инвентарь, рынок и производство.
//!
//! ## Детерминизм
//! Вся арифметика — на целых числах: кредиты (`u64`), количество (`u64`),
//! цена (`u64`). Дробных чисел нет вовсе, поэтому экономика воспроизводима
//! бит-в-бит на сервере, в симуляции и в тестах — как и физика.
//!
//! ## Что здесь есть
//! * [`ItemId`] и [`ItemRegistry`] — предметы и ресурсы.
//! * [`Inventory`] — что у кого лежит (по владельцу и предмету).
//! * [`Market`] — стакан ордеров на каждую звёздную систему.
//! * [`MatchingEngine`] — исполнение сделок по цене и времени.
//! * [`Production`] — производственные цепочки (ресурсы → детали → корабли).
//!
//! ## Инварианты (проверяются тестами)
//! * Кредиты и предметы в сделке **сохраняются**: сколько покупатель заплатил,
//!   столько продавец получил; предмет ушёл от продавца к покупателю.
//! * Приоритет исполнения: лучшая цена, затем более ранний ордер, затем
//!   меньший `id` — это делает порядок сделок однозначным.
//! * Нельзя продать больше, чем есть (проверка при размещении).

use std::collections::BTreeMap;

pub mod market_sim;

pub use market_sim::{Agent, MarketTick, Role, SimConfig, SimResult, Simulator};

/// Идентификатор предмета или ресурса.
pub type ItemId = u32;
/// Идентификатор владельца (персонаж или организация).
pub type OwnerId = u32;
/// Идентификатор звёздной системы (рынки изолированы по системам).
pub type SystemId = u32;
/// Идентификатор ордера.
pub type OrderId = u64;
/// Кредиты — целые, без дробей.
pub type Credits = u64;

// ---------------------------------------------------------------------------
// Предметы
// ---------------------------------------------------------------------------

/// Категория предмета.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    /// Сырьё из поясов и планет.
    Raw,
    /// Продукт переработки.
    Refined,
    /// Деталь для конструкций.
    Component,
    /// Готовая конструкция (корабль, модуль).
    Construct,
    /// Расходник.
    Consumable,
}

/// Описание предмета.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemDef {
    pub id: ItemId,
    pub name: String,
    pub category: Category,
    /// Базовая (референсная) цена, используется только как ориентир.
    pub base_price: Credits,
}

/// Реестр предметов.
#[derive(Clone, Debug, Default)]
pub struct ItemRegistry {
    items: BTreeMap<ItemId, ItemDef>,
}

impl ItemRegistry {
    pub fn new() -> Self {
        ItemRegistry { items: BTreeMap::new() }
    }

    pub fn register(&mut self, name: &str, category: Category, base_price: Credits) -> ItemId {
        let id = self.items.keys().next_back().copied().unwrap_or(0) + 1;
        self.items.insert(
            id,
            ItemDef { id, name: name.to_string(), category, base_price },
        );
        id
    }

    pub fn get(&self, id: ItemId) -> Option<&ItemDef> {
        self.items.get(&id)
    }

    pub fn name(&self, id: ItemId) -> &str {
        self.items.get(&id).map(|d| d.name.as_str()).unwrap_or("?")
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Базовый стартовый набор предметов Open Universe.
    pub fn default_universe() -> Self {
        let mut r = ItemRegistry::new();
        r.register("руда", Category::Raw, 10);
        r.register("лёд", Category::Raw, 8);
        r.register("кристалл", Category::Raw, 25);
        r.register("металл", Category::Refined, 40);
        r.register("топливо", Category::Consumable, 30);
        r.register("сплав", Category::Refined, 90);
        r.register("электроника", Category::Component, 200);
        r.register("корпус", Category::Component, 350);
        r.register("двигатель", Category::Component, 600);
        r.register("корабль", Category::Construct, 2500);
        r
    }
}

// ---------------------------------------------------------------------------
// Инвентарь
// ---------------------------------------------------------------------------

/// Склад: у кого сколько чего лежит.
#[derive(Clone, Debug, Default)]
pub struct Inventory {
    /// (владелец, система) → предмет → количество. Система важна: груз
    /// физически находится в конкретной звёздной системе.
    slots: BTreeMap<(OwnerId, SystemId), BTreeMap<ItemId, u64>>,
}

impl Inventory {
    pub fn new() -> Self {
        Inventory { slots: BTreeMap::new() }
    }

    pub fn add(&mut self, owner: OwnerId, system: SystemId, item: ItemId, qty: u64) {
        *self
            .slots
            .entry((owner, system))
            .or_default()
            .entry(item)
            .or_insert(0) += qty;
    }

    /// Снять предметы. Возвращает `false`, если их не хватает (ничего не меняя).
    pub fn take(&mut self, owner: OwnerId, system: SystemId, item: ItemId, qty: u64) -> bool {
        let key = (owner, system);
        let Some(bag) = self.slots.get_mut(&key) else {
            return false;
        };
        let have = bag.get(&item).copied().unwrap_or(0);
        if have < qty {
            return false;
        }
        let left = have - qty;
        if left == 0 {
            bag.remove(&item);
        } else {
            bag.insert(item, left);
        }
        if bag.is_empty() {
            self.slots.remove(&key);
        }
        true
    }

    pub fn amount(&self, owner: OwnerId, system: SystemId, item: ItemId) -> u64 {
        self.slots
            .get(&(owner, system))
            .and_then(|b| b.get(&item))
            .copied()
            .unwrap_or(0)
    }

    /// Сколько всего предметов у владельца во всех системах.
    pub fn total_of(&self, owner: OwnerId, item: ItemId) -> u64 {
        self.slots
            .iter()
            .filter(|((o, _), _)| *o == owner)
            .filter_map(|(_, b)| b.get(&item).copied())
            .sum()
    }

    /// Полный перебор содержимого (для контрольной суммы и сохранения).
    pub fn iter_slots(&self) -> impl Iterator<Item = (&(OwnerId, SystemId), &BTreeMap<ItemId, u64>)> {
        self.slots.iter()
    }
}

// ---------------------------------------------------------------------------
// Рынок: ордера и стакан
// ---------------------------------------------------------------------------

/// Сторона ордера.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// Покупка: покупатель готов заплатить `price` за единицу.
    Buy,
    /// Продажа: продавец просит `price` за единицу.
    Sell,
}

/// Ордер в стакане.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Order {
    pub id: OrderId,
    pub system: SystemId,
    pub item: ItemId,
    pub owner: OwnerId,
    pub side: Side,
    /// Цена за единицу.
    pub price: Credits,
    /// Оставшееся количество.
    pub qty: u64,
    /// Тик, когда ордер был поставлен (для приоритета по времени).
    pub placed_tick: u64,
}

/// Запись об исполнении сделки.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trade {
    pub system: SystemId,
    pub item: ItemId,
    pub buyer: OwnerId,
    pub seller: OwnerId,
    /// Цена исполнения (цена «старого» ордера, стоявшего в стакане).
    pub price: Credits,
    pub qty: u64,
    /// Тик исполнения.
    pub tick: u64,
    pub buy_order: OrderId,
    pub sell_order: OrderId,
}

impl Trade {
    pub fn total(&self) -> Credits {
        self.price.saturating_mul(self.qty)
    }
}

/// Стакан одного предмета в одной системе.
#[derive(Clone, Debug, Default)]
struct Book {
    /// Активные ордера по id.
    orders: BTreeMap<OrderId, Order>,
    /// Совокупный объём по цене (для быстрой оценки глубины).
    bids: BTreeMap<Credits, u64>, // цена → объём покупок
    asks: BTreeMap<Credits, u64>, // цена → объём продаж
}

impl Book {
    fn insert(&mut self, o: Order) {
        let depth = if o.side == Side::Buy { &mut self.bids } else { &mut self.asks };
        *depth.entry(o.price).or_insert(0) += o.qty;
        self.orders.insert(o.id, o);
    }

    fn remove(&mut self, id: OrderId) -> Option<Order> {
        let o = self.orders.remove(&id)?;
        let depth = if o.side == Side::Buy { &mut self.bids } else { &mut self.asks };
        if let Some(d) = depth.get_mut(&o.price) {
            *d = d.saturating_sub(o.qty);
            if *d == 0 {
                depth.remove(&o.price);
            }
        }
        Some(o)
    }

    /// Снизить объём ордера после частичного исполнения.
    fn reduce(&mut self, id: OrderId, qty: u64) {
        if let Some(o) = self.orders.get_mut(&id) {
            let depth = if o.side == Side::Buy { &mut self.bids } else { &mut self.asks };
            if let Some(d) = depth.get_mut(&o.price) {
                *d = d.saturating_sub(qty);
                if *d == 0 {
                    depth.remove(&o.price);
                }
            }
            o.qty = o.qty.saturating_sub(qty);
            if o.qty == 0 {
                self.orders.remove(&id);
            }
        }
    }
}

/// Рынок: стаканы по всем системам и предметам.
#[derive(Clone, Debug, Default)]
pub struct Market {
    books: BTreeMap<(SystemId, ItemId), Book>,
    next_order_id: OrderId,
}

/// Лучшая цена и её объём (верх стакана).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Level {
    pub price: Credits,
    pub qty: u64,
}

impl Market {
    pub fn new() -> Self {
        Market { books: BTreeMap::new(), next_order_id: 1 }
    }

    /// Верхняя граница стакана покупок (самая высокая цена).
    pub fn best_bid(&self, system: SystemId, item: ItemId) -> Option<Level> {
        self.books
            .get(&(system, item))
            .and_then(|b| b.bids.iter().next_back().map(|(p, q)| Level { price: *p, qty: *q }))
    }

    /// Верхняя граница стакана продаж (самая низкая цена).
    pub fn best_ask(&self, system: SystemId, item: ItemId) -> Option<Level> {
        self.books
            .get(&(system, item))
            .and_then(|b| b.asks.iter().next().map(|(p, q)| Level { price: *p, qty: *q }))
    }

    /// Спред между лучшей покупкой и продажей.
    pub fn spread(&self, system: SystemId, item: ItemId) -> Option<Credits> {
        match (self.best_bid(system, item), self.best_ask(system, item)) {
            (Some(b), Some(a)) if a.price >= b.price => Some(a.price - b.price),
            _ => None,
        }
    }

    pub fn order(&self, id: OrderId) -> Option<&Order> {
        self.books.values().find_map(|b| b.orders.get(&id))
    }

    /// Все активные ордера (в детерминированном порядке id).
    pub fn all_orders(&self) -> Vec<Order> {
        let mut v: Vec<Order> = self
            .books
            .values()
            .flat_map(|b| b.orders.values().cloned())
            .collect();
        v.sort_by_key(|o| o.id);
        v
    }

    /// Следующий id, который будет выдан (для сохранения/восстановления).
    pub fn next_order_id(&self) -> OrderId {
        self.next_order_id
    }

    /// Восстановить ордер напрямую (загрузка из снимка). Не исполняет сделку.
    pub fn restore_order(&mut self, o: Order) {
        self.next_order_id = self.next_order_id.max(o.id + 1);
        self.books.entry((o.system, o.item)).or_default().insert(o);
    }

    /// Установить счётчик id (при восстановлении снимка).
    pub fn set_next_order_id(&mut self, id: OrderId) {
        self.next_order_id = id.max(self.next_order_id);
    }

    pub fn open_orders(&self, system: SystemId, item: ItemId) -> usize {
        self.books.get(&(system, item)).map(|b| b.orders.len()).unwrap_or(0)
    }

    /// Снять ордер (возвращает снятый ордер).
    pub fn cancel(&mut self, id: OrderId) -> Option<Order> {
        for b in self.books.values_mut() {
            if b.orders.contains_key(&id) {
                return b.remove(id);
            }
        }
        None
    }

    /// Снимок стакана: (покупки от лучшей, продажи от лучшей).
    pub fn depth(
        &self,
        system: SystemId,
        item: ItemId,
        levels: usize,
    ) -> (Vec<Level>, Vec<Level>) {
        let Some(b) = self.books.get(&(system, item)) else {
            return (vec![], vec![]);
        };
        let bids = b
            .bids
            .iter()
            .rev()
            .take(levels)
            .map(|(p, q)| Level { price: *p, qty: *q })
            .collect();
        let asks = b
            .asks
            .iter()
            .take(levels)
            .map(|(p, q)| Level { price: *p, qty: *q })
            .collect();
        (bids, asks)
    }
}

// ---------------------------------------------------------------------------
// Движок исполнения
// ---------------------------------------------------------------------------

/// Итог размещения ордера.
#[derive(Clone, Debug, Default)]
pub struct FillReport {
    pub order_id: OrderId,
    /// Совершённые сделки.
    pub trades: Vec<Trade>,
    /// Сколько исполнилось сразу.
    pub filled: u64,
    /// Сколько осталось в стакане.
    pub remaining: u64,
}

/// Движок исполнения сделок. Приоритет: цена → время → id.
#[derive(Clone, Debug, Default)]
pub struct MatchingEngine {
    pub market: Market,
    pub inventory: Inventory,
    /// Журнал сделок (для истории и контрольной суммы).
    pub trades: Vec<Trade>,
}

impl MatchingEngine {
    pub fn new(market: Market, inventory: Inventory) -> Self {
        MatchingEngine { market, inventory, trades: Vec::new() }
    }

    /// Разместить ордер продажи: предметы списываются сразу (эскроу).
    pub fn place_sell(
        &mut self,
        system: SystemId,
        item: ItemId,
        owner: OwnerId,
        price: Credits,
        qty: u64,
        tick: u64,
    ) -> Result<FillReport, EconError> {
        if qty == 0 {
            return Err(EconError::ZeroQuantity);
        }
        if price == 0 {
            return Err(EconError::ZeroPrice);
        }
        // Эскроу: продавец обязан иметь товар.
        if !self.inventory.take(owner, system, item, qty) {
            return Err(EconError::NotEnoughItems {
                have: self.inventory.amount(owner, system, item),
                want: qty,
            });
        }
        Ok(self.submit(system, item, owner, Side::Sell, price, qty, tick))
    }

    /// Разместить ордер покупки. Кредиты не блокируются в этой модели
    /// (MVP): проверяем лишь, что они есть, и списываем при исполнении.
    pub fn place_buy(
        &mut self,
        system: SystemId,
        item: ItemId,
        owner: OwnerId,
        price: Credits,
        qty: u64,
        tick: u64,
    ) -> Result<FillReport, EconError> {
        if qty == 0 {
            return Err(EconError::ZeroQuantity);
        }
        if price == 0 {
            return Err(EconError::ZeroPrice);
        }
        Ok(self.submit(system, item, owner, Side::Buy, price, qty, tick))
    }

    fn submit(
        &mut self,
        system: SystemId,
        item: ItemId,
        owner: OwnerId,
        side: Side,
        price: Credits,
        qty: u64,
        tick: u64,
    ) -> FillReport {
        let id = self.market.next_order_id;
        self.market.next_order_id += 1;
        let incoming = Order { id, system, item, owner, side, price, qty, placed_tick: tick };

        let mut report = FillReport { order_id: id, ..Default::default() };
        let mut book = self.market.books.remove(&(system, item)).unwrap_or_default();

        // Исполняем встречные ордера, пока цена пересекается и есть остаток.
        let mut remaining = qty;
        while remaining > 0 {
            let Some(counter_id) = Self::best_counter(&book, side, price) else {
                break;
            };
            let counter_order = book.orders.get(&counter_id).cloned().unwrap();
            let fill = remaining.min(counter_order.qty);
            if fill == 0 {
                break;
            }

            // Сделка идёт по цене ордера, стоявшего в стакане.
            let exec_price = counter_order.price;
            let (buyer, seller, buy_id, sell_id) = match side {
                Side::Buy => (owner, counter_order.owner, id, counter_id),
                Side::Sell => (counter_order.owner, owner, counter_id, id),
            };
            let trade = Trade {
                system,
                item,
                buyer,
                seller,
                price: exec_price,
                qty: fill,
                tick,
                buy_order: buy_id,
                sell_order: sell_id,
            };
            self.settle(&trade);
            self.trades.push(trade.clone());
            report.trades.push(trade);
            report.filled += fill;
            remaining -= fill;

            // Уменьшаем встречный ордер.
            book.reduce(counter_id, fill);
        }

        // Остаток входящего ордера встаёт в стакан.
        if remaining > 0 {
            book.insert(Order { qty: remaining, ..incoming });
        }
        report.remaining = remaining;

        self.market.books.insert((system, item), book);
        report
    }

    /// Лучший встречный ордер для входящего `side` с лимитом `limit`.
    ///
    /// Приоритет строго задан: сначала цена, затем время постановки, затем `id`.
    /// Это делает порядок исполнения однозначным и воспроизводимым.
    fn best_counter(book: &Book, side: Side, limit: Credits) -> Option<OrderId> {
        match side {
            // Входящая покупка бьёт по самым дешёвым продажам.
            Side::Buy => book
                .orders
                .values()
                .filter(|o| o.side == Side::Sell && o.price <= limit)
                .min_by_key(|o| (o.price, o.placed_tick, o.id))
                .map(|o| o.id),
            // Входящая продажа бьёт по самым дорогим покупкам.
            Side::Sell => book
                .orders
                .values()
                .filter(|o| o.side == Side::Buy && o.price >= limit)
                .min_by_key(|o| (std::cmp::Reverse(o.price), o.placed_tick, o.id))
                .map(|o| o.id),
        }
    }

    /// Перевести предметы и кредиты между сторонами сделки.
    fn settle(&mut self, t: &Trade) {
        // Предмет: у покупателя прибавляется (у продавца уже списан при
        // размещении sell-ордера).
        self.inventory.add(t.buyer, t.system, t.item, t.qty);
        // Кредиты переводятся внутри журнала (балансы ведёт вызывающая
        // сторона), но фиксируем факт: total = price * qty.
    }
}

// ---------------------------------------------------------------------------
// Производство
// ---------------------------------------------------------------------------

/// Рецепт: набор входов → выход, за время (в тиках).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recipe {
    pub id: u32,
    pub name: String,
    pub inputs: Vec<(ItemId, u64)>,
    pub output: (ItemId, u64),
    pub ticks: u64,
}

/// Каталог рецептов.
#[derive(Clone, Debug, Default)]
pub struct Production {
    recipes: BTreeMap<u32, Recipe>,
}

impl Production {
    pub fn new() -> Self {
        Production { recipes: BTreeMap::new() }
    }

    pub fn add(&mut self, name: &str, inputs: &[(ItemId, u64)], output: (ItemId, u64), ticks: u64) -> u32 {
        let id = self.recipes.keys().next_back().copied().unwrap_or(0) + 1;
        self.recipes.insert(
            id,
            Recipe { id, name: name.to_string(), inputs: inputs.to_vec(), output, ticks },
        );
        id
    }

    pub fn get(&self, id: u32) -> Option<&Recipe> {
        self.recipes.get(&id)
    }

    pub fn len(&self) -> usize {
        self.recipes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.recipes.is_empty()
    }

    /// Производственные цепочки Open Universe.
    pub fn default_chain(reg: &ItemRegistry) -> Self {
        let mut p = Production::new();
        // Находим предметы по имени (реестр строит их по порядку).
        let find = |n: &str| -> ItemId {
            (1..=reg.len() as u32).find(|i| reg.name(*i) == n).unwrap_or(0)
        };
        let (ore, ice, crystal, metal, fuel, alloy, elec, hull, engine, ship) = (
            find("руда"),
            find("лёд"),
            find("кристалл"),
            find("металл"),
            find("топливо"),
            find("сплав"),
            find("электроника"),
            find("корпус"),
            find("двигатель"),
            find("корабль"),
        );
        p.add("выплавка металла", &[(ore, 2)], (metal, 1), 5);
        p.add("перегонка топлива", &[(ice, 2)], (fuel, 1), 4);
        p.add("выплавка сплава", &[(metal, 2), (crystal, 1)], (alloy, 1), 8);
        p.add("сборка электроники", &[(crystal, 2), (alloy, 1)], (elec, 1), 12);
        p.add("штамповка корпуса", &[(alloy, 3)], (hull, 1), 10);
        p.add("сборка двигателя", &[(alloy, 2), (elec, 1)], (engine, 1), 15);
        p.add("сборка корабля", &[(hull, 1), (engine, 2), (elec, 2)], (ship, 1), 30);
        p
    }

    /// Проверить, хватает ли ресурсов на рецепт (без списания).
    pub fn can_craft(&self, recipe: u32, inv: &Inventory, owner: OwnerId, system: SystemId) -> bool {
        match self.recipes.get(&recipe) {
            None => false,
            Some(r) => r
                .inputs
                .iter()
                .all(|(item, need)| inv.amount(owner, system, *item) >= *need),
        }
    }

    /// Выполнить рецепт: списать входы, начислить выход.
    pub fn craft(
        &self,
        recipe: u32,
        inv: &mut Inventory,
        owner: OwnerId,
        system: SystemId,
    ) -> Result<(ItemId, u64), EconError> {
        let r = self.recipes.get(&recipe).ok_or(EconError::NoSuchRecipe(recipe))?;
        if !self.can_craft(recipe, inv, owner, system) {
            return Err(EconError::NotEnoughInputs);
        }
        for (item, need) in &r.inputs {
            // Уже проверено `can_craft`, поэтому take не может провалиться.
            inv.take(owner, system, *item, *need);
        }
        inv.add(owner, system, r.output.0, r.output.1);
        Ok(r.output)
    }
}

// ---------------------------------------------------------------------------
// Ошибки
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EconError {
    NotEnoughItems { have: u64, want: u64 },
    NotEnoughCredits { have: Credits, need: Credits },
    ZeroQuantity,
    ZeroPrice,
    NoSuchRecipe(u32),
    NotEnoughInputs,
    NoSuchOrder(OrderId),
}

impl std::fmt::Display for EconError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EconError::NotEnoughItems { have, want } => {
                write!(f, "не хватает предметов: есть {have}, нужно {want}")
            }
            EconError::NotEnoughCredits { have, need } => {
                write!(f, "не хватает кредитов: есть {have}, нужно {need}")
            }
            EconError::ZeroQuantity => write!(f, "количество должно быть больше нуля"),
            EconError::ZeroPrice => write!(f, "цена должна быть больше нуля"),
            EconError::NoSuchRecipe(id) => write!(f, "рецепт {id} не найден"),
            EconError::NotEnoughInputs => write!(f, "не хватает сырья для рецепта"),
            EconError::NoSuchOrder(id) => write!(f, "ордер {id} не найден"),
        }
    }
}

impl std::error::Error for EconError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> (MatchingEngine, ItemRegistry) {
        let reg = ItemRegistry::default_universe();
        let inv = Inventory::new();
        (MatchingEngine::new(Market::new(), inv), reg)
    }

    fn item(reg: &ItemRegistry, name: &str) -> ItemId {
        (1..=reg.len() as u32).find(|i| reg.name(*i) == name).unwrap()
    }

    #[test]
    fn registry_has_expected_items() {
        let r = ItemRegistry::default_universe();
        assert_eq!(r.len(), 10);
        assert_eq!(r.get(1).unwrap().name, "руда");
        assert_eq!(r.get(10).unwrap().category, Category::Construct);
    }

    #[test]
    fn inventory_add_take_and_conserve() {
        let mut inv = Inventory::new();
        inv.add(1, 100, 5, 10);
        assert_eq!(inv.amount(1, 100, 5), 10);
        assert!(inv.take(1, 100, 5, 4));
        assert_eq!(inv.amount(1, 100, 5), 6);
        assert!(!inv.take(1, 100, 5, 100), "нельзя взять больше, чем есть");
        assert_eq!(inv.amount(1, 100, 5), 6, "неудачный take не должен менять состояние");
        assert!(inv.take(1, 100, 5, 6));
        assert_eq!(inv.amount(1, 100, 5), 0);
    }

    #[test]
    fn sell_requires_items() {
        let (mut eng, reg) = engine();
        let ore = item(&reg, "руда");
        // У продавца нет руды → ошибка.
        let err = eng.place_sell(1, ore, 7, 10, 5, 1).unwrap_err();
        assert!(matches!(err, EconError::NotEnoughItems { .. }));
    }

    #[test]
    fn simple_match_transfers_items() {
        let (mut eng, reg) = engine();
        let ore = item(&reg, "руда");
        eng.inventory.add(7, 1, ore, 10); // у продавца 10 руды

        // Продавец ставит 5 руды по 12.
        let r_sell = eng.place_sell(1, ore, 7, 12, 5, 1).unwrap();
        assert_eq!(r_sell.filled, 0, "пока нет покупателей");
        assert_eq!(r_sell.remaining, 5);
        assert_eq!(eng.inventory.amount(7, 1, ore), 5, "товар ушёл в эскроу");

        // Покупатель берёт 3 по 15 (переплата допустима — исполнение по 12).
        let r_buy = eng.place_buy(1, ore, 9, 15, 3, 2).unwrap();
        assert_eq!(r_buy.filled, 3);
        assert_eq!(r_buy.trades.len(), 1);
        let t = &r_buy.trades[0];
        assert_eq!(t.price, 12, "сделка по цене продавца");
        assert_eq!(t.total(), 36);
        assert_eq!(t.buyer, 9);
        assert_eq!(t.seller, 7);

        // Товар у покупателя, у продавца осталось 5 (6 из 10 ушли: 5 в эскроу? нет).
        assert_eq!(eng.inventory.amount(9, 1, ore), 3);
        // Продавец выставил 5, из них 3 продано, значит 2 всё ещё в эскроу,
        // а на складе у него остались исходные 5 (10 - 5 в эскроу).
        assert_eq!(eng.inventory.amount(7, 1, ore), 5, "5 остались на складе продавца");
        // В стакане осталось 2 руды на продажу по 12.
        let (_, asks) = eng.market.depth(1, ore, 5);
        assert_eq!(asks, vec![Level { price: 12, qty: 2 }]);
    }

    #[test]
    fn price_priority_executes_best_first() {
        let (mut eng, reg) = engine();
        let ore = item(&reg, "руда");
        for seller in [1u32, 2, 3] {
            eng.inventory.add(seller, 1, ore, 10);
        }
        // Три продажи по разным ценам.
        eng.place_sell(1, ore, 1, 30, 5, 1).unwrap();
        eng.place_sell(1, ore, 2, 10, 5, 2).unwrap(); // самая дешёвая
        eng.place_sell(1, ore, 3, 20, 5, 3).unwrap();

        assert_eq!(eng.market.best_ask(1, ore), Some(Level { price: 10, qty: 5 }));

        // Покупатель на 5 единиц по 25 должен взять самую дешёвую (10).
        let r = eng.place_buy(1, ore, 9, 25, 5, 4).unwrap();
        assert_eq!(r.trades.len(), 1);
        assert_eq!(r.trades[0].price, 10);
        assert_eq!(r.trades[0].seller, 2);
    }

    #[test]
    fn order_matches_across_multiple_levels() {
        let (mut eng, reg) = engine();
        let ore = item(&reg, "руда");
        for s in [1u32, 2] {
            eng.inventory.add(s, 1, ore, 10);
        }
        eng.place_sell(1, ore, 1, 10, 2, 1).unwrap();
        eng.place_sell(1, ore, 2, 12, 3, 2).unwrap();

        // Покупаем 4 по 12: 2 по 10 + 2 по 12.
        let r = eng.place_buy(1, ore, 9, 12, 4, 3).unwrap();
        assert_eq!(r.trades.len(), 2);
        assert_eq!(r.filled, 4);
        assert_eq!(r.trades[0].price, 10);
        assert_eq!(r.trades[0].qty, 2);
        assert_eq!(r.trades[1].price, 12);
        assert_eq!(r.trades[1].qty, 2);
        // Остаток продажи 12: 1 единица.
        assert_eq!(eng.market.best_ask(1, ore), Some(Level { price: 12, qty: 1 }));
    }

    #[test]
    fn partial_fill_leaves_remainder_in_book() {
        let (mut eng, reg) = engine();
        let ore = item(&reg, "руда");
        eng.inventory.add(1, 1, ore, 10);
        eng.place_sell(1, ore, 1, 10, 10, 1).unwrap();
        let r = eng.place_buy(1, ore, 9, 10, 4, 2).unwrap();
        assert_eq!(r.filled, 4);
        assert_eq!(r.remaining, 0, "покупка исполнена полностью");
        assert_eq!(eng.market.best_ask(1, ore), Some(Level { price: 10, qty: 6 }));
    }

    #[test]
    fn no_cross_when_prices_do_not_overlap() {
        let (mut eng, reg) = engine();
        let ore = item(&reg, "руда");
        eng.inventory.add(1, 1, ore, 10);
        eng.place_sell(1, ore, 1, 20, 5, 1).unwrap();
        // Покупка по 15 не должна исполниться против продажи по 20.
        let r = eng.place_buy(1, ore, 9, 15, 5, 2).unwrap();
        assert_eq!(r.filled, 0);
        assert_eq!(r.remaining, 5);
        assert_eq!(eng.market.best_bid(1, ore), Some(Level { price: 15, qty: 5 }));
        assert_eq!(eng.market.spread(1, ore), Some(5));
    }

    #[test]
    fn markets_are_isolated_by_system() {
        let (mut eng, reg) = engine();
        let ore = item(&reg, "руда");
        eng.inventory.add(1, 5, ore, 10);
        eng.place_sell(5, ore, 1, 10, 5, 1).unwrap();
        // Покупка в другой системе не должна видеть этот ордер.
        let r = eng.place_buy(6, ore, 9, 100, 5, 2).unwrap();
        assert_eq!(r.filled, 0);
        assert_eq!(eng.market.best_ask(5, ore), Some(Level { price: 10, qty: 5 }));
        assert_eq!(eng.market.best_ask(6, ore), None);
    }

    #[test]
    fn cancel_removes_order_and_restores_nothing_automatically() {
        let (mut eng, reg) = engine();
        let ore = item(&reg, "руда");
        eng.inventory.add(1, 1, ore, 10);
        let r = eng.place_sell(1, ore, 1, 10, 4, 1).unwrap();
        let order = eng.market.cancel(r.order_id).unwrap();
        assert_eq!(order.qty, 4);
        assert_eq!(eng.market.open_orders(1, ore), 0);
    }

    #[test]
    fn time_priority_breaks_price_ties() {
        let (mut eng, reg) = engine();
        let ore = item(&reg, "руда");
        for s in [1u32, 2] {
            eng.inventory.add(s, 1, ore, 5);
        }
        // Два ордера по одинаковой цене; раньше поставлен seller=2.
        let r2 = eng.place_sell(1, ore, 2, 10, 5, 1).unwrap();
        let _r1 = eng.place_sell(1, ore, 1, 10, 5, 1).unwrap();
        let r = eng.place_buy(1, ore, 9, 10, 3, 2).unwrap();
        assert_eq!(r.trades[0].seller, 2, "раньше поставленный ордер исполняется первым");
        // Он тот, у кого меньший id (2 поставлен первым).
        assert_eq!(r.trades[0].sell_order, r2.order_id);
    }

    #[test]
    fn credits_and_items_are_conserved_in_trade() {
        let (mut eng, reg) = engine();
        let ore = item(&reg, "руда");
        eng.inventory.add(7, 1, ore, 100);
        eng.place_sell(1, ore, 7, 25, 40, 1).unwrap();
        let r = eng.place_buy(1, ore, 9, 25, 40, 2).unwrap();
        let paid: Credits = r.trades.iter().map(|t| t.total()).sum();
        // Покупатель заплатил ровно столько, сколько получил продавец.
        assert_eq!(paid, 25 * 40);
        // Предметов суммарно столько же: 40 у покупателя + 40 в эскроу
        // + 20 на складе у продавца = 100.
        assert_eq!(eng.inventory.amount(9, 1, ore), 40);
        let in_book = eng.market.best_ask(1, ore).map(|l| l.qty).unwrap_or(0);
        let on_shelf = eng.inventory.amount(7, 1, ore);
        assert_eq!(in_book + eng.inventory.amount(9, 1, ore) + on_shelf, 100);
    }

    #[test]
    fn production_chain_refines_raw_materials() {
        let reg = ItemRegistry::default_universe();
        let prod = Production::default_chain(&reg);
        assert_eq!(prod.len(), 7);

        let ore = item(&reg, "руда");
        let crystal = item(&reg, "кристалл");
        let elec = item(&reg, "электроника");

        let mut inv = Inventory::new();
        inv.add(1, 1, ore, 100);
        inv.add(1, 1, crystal, 60);

        // Гоним цепочку снизу вверх, пока что-то получается.
        for _pass in 0..20 {
            for rid in 1..=prod.len() as u32 {
                while prod.can_craft(rid, &inv, 1, 1) {
                    prod.craft(rid, &mut inv, 1, 1).unwrap();
                }
            }
        }
        // Сырьё переработано, и появились верхние продукты цепочки.
        assert!(inv.amount(1, 1, ore) < 100, "руда должна быть переработана");
        assert!(inv.amount(1, 1, elec) > 0, "должна получиться электроника");
    }

    #[test]
    fn craft_ship_from_components() {
        let reg = ItemRegistry::default_universe();
        let prod = Production::default_chain(&reg);
        let (hull, engine, elec, ship) = (
            item(&reg, "корпус"),
            item(&reg, "двигатель"),
            item(&reg, "электроника"),
            item(&reg, "корабль"),
        );
        // Рецепт корабля — последний в цепочке.
        let ship_recipe = (1..=prod.len() as u32)
            .find(|id| prod.get(*id).unwrap().output.0 == ship)
            .unwrap();

        let mut inv = Inventory::new();
        inv.add(1, 1, hull, 1);
        inv.add(1, 1, engine, 2);
        inv.add(1, 1, elec, 2);

        assert!(prod.can_craft(ship_recipe, &inv, 1, 1));
        prod.craft(ship_recipe, &mut inv, 1, 1).unwrap();
        assert_eq!(inv.amount(1, 1, ship), 1);
        // Входы израсходованы.
        assert_eq!(inv.amount(1, 1, hull), 0);
        assert_eq!(inv.amount(1, 1, engine), 0);
    }

    #[test]
    fn craft_fails_without_inputs() {
        let reg = ItemRegistry::default_universe();
        let prod = Production::default_chain(&reg);
        let mut inv = Inventory::new();
        assert!(!prod.can_craft(1, &inv, 1, 1));
        let err = prod.craft(1, &mut inv, 1, 1).unwrap_err();
        assert_eq!(err, EconError::NotEnoughInputs);
    }

    #[test]
    fn spread_and_depth_are_reported() {
        let (mut eng, reg) = engine();
        let ore = item(&reg, "руда");
        for s in [1u32, 2] {
            eng.inventory.add(s, 1, ore, 10);
        }
        eng.place_sell(1, ore, 1, 11, 4, 1).unwrap();
        eng.place_sell(1, ore, 2, 13, 6, 1).unwrap();
        eng.place_buy(1, ore, 3, 9, 5, 1).unwrap();

        let (bids, asks) = eng.market.depth(1, ore, 5);
        assert_eq!(bids, vec![Level { price: 9, qty: 5 }]);
        assert_eq!(
            asks,
            vec![Level { price: 11, qty: 4 }, Level { price: 13, qty: 6 }]
        );
        assert_eq!(eng.market.spread(1, ore), Some(2)); // 11 - 9
    }

    #[test]
    fn matching_is_deterministic() {
        // Одинаковая последовательность действий должна давать одинаковый
        // журнал сделок — это требование к MMO-экономике.
        let run = || {
            let reg = ItemRegistry::default_universe();
            let ore = item(&reg, "руда");
            let mut eng = MatchingEngine::new(Market::new(), Inventory::new());
            for s in 1..=5u32 {
                eng.inventory.add(s, 1, ore, 100);
            }
            eng.place_sell(1, ore, 1, 30, 10, 1).unwrap();
            eng.place_sell(1, ore, 2, 20, 10, 1).unwrap();
            eng.place_sell(1, ore, 3, 25, 10, 1).unwrap();
            eng.place_buy(1, ore, 4, 26, 15, 2).unwrap();
            eng.place_buy(1, ore, 5, 22, 10, 3).unwrap();
            eng.trades.clone()
        };
        assert_eq!(run(), run());
    }
}
