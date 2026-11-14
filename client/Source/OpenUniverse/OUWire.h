// Copyright Open Universe. Все права защищены.
//
// OUWire — разбор бинарного протокола Open Universe.
//
// ## Почему отдельный заголовок
// Формат провода — это контракт между Rust-сервером и C++-клиентом. Логика
// разбора не должна зависеть от Unreal: тогда её можно проверить обычным
// компилятором на реальных пакетах сервера (`tools/test-client.ps1`).
//
// ## Зависимости
// Только стандартная библиотека (`<cstdint>`, `<string>`, `<vector>`,
// `<cstring>`). Никаких UE-типов — поэтому заголовок собирается и вне движка.
//
// ## Формат (совпадает с `server/crates/protocol`)
//   пакет: [token: u64 LE][len: u32 LE][body...]
//   body:  [tag: u8][...]
//   tag 5 (snapshot v2):
//     tick(varint), ack(varint), baseline(u8), count(varint),
//     затем на сущность:
//       id(varint), pos(3×zigzag varint), vel(3×zigzag varint),
//       yaw(f32 LE), pitch(f32 LE)
//
// Масштаб позиции на сервере: 1 единица провода = 1e-4 игровых единиц.
// В UE 1 юнит = 1 см, а игровая единица трактуется как метр (100 см),
// поэтому единица провода = 1e-4 × 100 = 0.01 см.

#pragma once

#include <cstdint>
#include <cmath>
#include <cstring>
#include <string>
#include <vector>

namespace OUWire
{
    // --- Константы формата (должны совпадать с Rust) ---

    /// Теги сообщений.
    enum Tag : uint8_t
    {
        TagHello = 1,
        TagInput = 2,
        TagSnapshot = 3,
        TagAck = 4,
        TagSnapshotV2 = 5,
        TagBuild = 6,
        TagBuildResult = 7,
        TagWelcome = 8,
        TagBuildEdit = 9,
        TagBuildAck = 10,
        TagBuildGrant = 11,
        TagBuildAccess = 12,
        TagShot = 13,
        TagHull = 14,
        TagDeath = 15,
        TagRespawn = 16,
        TagClientCharacter = 17,
        TagServerCharacter = 18,
        TagSystem = 19,
        TagDig = 20,
        TagMarketOrder = 21,
        TagMarketState = 22,
        TagDigYield = 23,
        TagPing = 24,
        TagPong = 25,
        TagRecipes = 26,
        TagCraft = 27,
        TagCraftResult = 28,
        TagTerritory = 29,
        TagCapture = 30,
        TagSetScript = 31,
        TagScript = 32,
        TagScripts = 33,
        TagPlaceBlueprint = 34,
        TagBlueprints = 35,
        TagCombat = 36,
        TagPublishBlueprint = 37,
        TagBuyBlueprint = 38,
        TagPlaceOwnBlueprint = 39,
        TagWorkshop = 40,
        TagWorkshopResult = 41,
    };

    /// Действия редактора конструкций.
    enum BuildAction : uint8_t
    {
        BuildPlace = 0,
        BuildRemove = 1,
        BuildPaint = 2,
    };

    /// Причины отказа строительства.
    enum BuildReason : uint8_t
    {
        BuildOk = 0,
        BuildInvalid = 1,
        BuildDisconnected = 2,
        BuildLimitReached = 3,
    };

    /// Масштаб позиции: 1 единица провода = 1e-3 игровых единиц (1 мм).
    /// Диапазон i32 — ±2147 км: этого хватает для планеты радиусом 500 км.
    constexpr double WireScale = 1.0 / 1000.0;
    /// Игровая единица = 1 метр = 100 см UE.
    constexpr double WorldToCm = 100.0;
    /// Итоговый множитель «провод → см UE».
    constexpr double WireToCm = WireScale * WorldToCm;

    /// Максимальный размер дейтаграммы.
    constexpr uint32_t MaxDatagram = 1200;

    // --- Типы результата (без UE) ---

    /// Состояние одной сущности, уже в сантиметрах UE.
    struct EntityState
    {
        int32_t Id = 0;
        float X = 0.f, Y = 0.f, Z = 0.f;
        float VX = 0.f, VY = 0.f, VZ = 0.f;
        float Yaw = 0.f, Pitch = 0.f;
    };

    /// Разобранный снимок.
    struct Snapshot
    {
        uint64_t Tick = 0;
        int32_t AckInputSeq = 0;
        bool Baseline = false;
        std::vector<EntityState> Entities;
    };

    /// Команда строительства (клиент → сервер).
    struct BuildCommand
    {
        uint32_t Entity = 0;
        BuildAction Action = BuildPlace;
        int32_t X = 0, Y = 0, Z = 0;
        uint8_t R = 0, G = 0, B = 0;
    };

    /// Результат строительства (сервер → клиент).
    struct BuildResult
    {
        bool Accepted = false;
        BuildReason Reason = BuildOk;
    };

    /// Применённая правка конструкции (сервер → клиент, тег 9).
    struct BuildEdit
    {
        uint32_t Entity = 0;
        uint32_t Author = 0;
        BuildAction Action = BuildPlace;
        int32_t X = 0, Y = 0, Z = 0;
        uint8_t R = 0, G = 0, B = 0;
        uint64_t Revision = 0;
    };

    /// Права на редактирование конструкции (сервер → клиент, тег 12).
    struct BuildAccess
    {
        uint32_t Entity = 0;
        bool CanEdit = false;
    };

    /// Состоявшийся выстрел (сервер → клиент, тег 13).
    struct ShotEvent
    {
        uint32_t Attacker = 0;
        uint32_t Target = 0;   // 0 — промах без цели
        bool Hit = false;
        uint32_t Damage = 0;
        bool Destroyed = false;
    };

    /// Текущее состояние корпуса сущности (сервер → клиент, тег 14).
    struct HullState
    {
        uint32_t Entity = 0;
        uint32_t Shield = 0;
        uint32_t Armor = 0;
        bool Destroyed = false;
    };

    /// Смерть сущности (сервер → клиент, тег 15).
    struct DeathEvent
    {
        uint32_t Entity = 0;
        uint64_t RespawnTick = 0;
    };

    /// Возрождение сущности (сервер → клиент, тег 16).
    struct RespawnEvent
    {
        uint32_t Entity = 0;
        uint64_t InvulnUntil = 0;
    };

    /// Поза чужого пешего персонажа (сервер → клиент, тег 18).
    struct CharacterPose
    {
        uint32_t Entity = 0;
        float X = 0.f, Y = 0.f, Z = 0.f;
        float Yaw = 0.f;
        bool Moving = false;
    };

    /// Тело звёздной системы (сервер → клиент, тег 19).
    struct SystemBody
    {
        uint32_t Id = 0;
        uint8_t Kind = 0;
        uint8_t Surface = 0;               // тип поверхности (палитра)
        float CenterX = 0.f, CenterY = 0.f, CenterZ = 0.f;   // центр орбиты, серверные ед.
        float Orbit = 0.f;                 // радиус орбиты, серверные ед. (0 — неподвижно)
        float Phase = 0.f;                 // начальная фаза, рад
        float Omega = 0.f;                 // угловая скорость, рад/с
        double Radius = 0.0;               // радиус тела, серверные единицы
        double Amplitude = 0.0;            // амплитуда рельефа, серверные единицы
        uint64_t Seed = 0;                 // сид рельефа (общая формула с сервером)
    };

    /// Описание звёздной системы (сервер → клиент, тег 19).
    struct SystemInfo
    {
        uint32_t Id = 0;
        std::string Name;
        std::vector<SystemBody> Bodies;
    };

    // --- Низкоуровневое чтение ---

    /// Курсор по буферу.
    struct Reader
    {
        const uint8_t* Data = nullptr;
        size_t Len = 0;
        size_t Pos = 0;

        bool Eof() const { return Pos >= Len; }

        bool Take(size_t n, const uint8_t*& out)
        {
            if (Pos + n > Len) return false;
            out = Data + Pos;
            Pos += n;
            return true;
        }

        bool U8(uint8_t& v)
        {
            const uint8_t* p = nullptr;
            if (!Take(1, p)) return false;
            v = p[0];
            return true;
        }

        bool U32(uint32_t& v)
        {
            const uint8_t* p = nullptr;
            if (!Take(4, p)) return false;
            v = static_cast<uint32_t>(p[0]) | (static_cast<uint32_t>(p[1]) << 8)
              | (static_cast<uint32_t>(p[2]) << 16) | (static_cast<uint32_t>(p[3]) << 24);
            return true;
        }

        bool F32(float& v)
        {
            const uint8_t* p = nullptr;
            if (!Take(4, p)) return false;
            uint32_t bits = static_cast<uint32_t>(p[0]) | (static_cast<uint32_t>(p[1]) << 8)
                          | (static_cast<uint32_t>(p[2]) << 16) | (static_cast<uint32_t>(p[3]) << 24);
            std::memcpy(&v, &bits, 4);
            return true;
        }

        bool F64(double& v)
        {
            const uint8_t* p = nullptr;
            if (!Take(8, p)) return false;
            uint64_t bits = 0;
            for (int i = 0; i < 8; ++i) { bits |= static_cast<uint64_t>(p[i]) << (8 * i); }
            std::memcpy(&v, &bits, 8);
            return true;
        }

        bool I32(int32_t& v)
        {
            const uint8_t* p = nullptr;
            if (!Take(4, p)) return false;
            uint32_t u = static_cast<uint32_t>(p[0]) | (static_cast<uint32_t>(p[1]) << 8)
                       | (static_cast<uint32_t>(p[2]) << 16) | (static_cast<uint32_t>(p[3]) << 24);
            v = static_cast<int32_t>(u);
            return true;
        }

        /// Varint (LEB128).
        bool Varint(uint64_t& v)
        {
            uint64_t result = 0;
            int shift = 0;
            while (Pos < Len)
            {
                const uint8_t b = Data[Pos++];
                result |= static_cast<uint64_t>(b & 0x7F) << shift;
                if ((b & 0x80) == 0)
                {
                    v = result;
                    return true;
                }
                shift += 7;
                if (shift >= 64) return false;
            }
            return false;
        }

        /// Знаковый varint (zigzag).
        bool SVarint(int64_t& v)
        {
            uint64_t zz = 0;
            if (!Varint(zz)) return false;
            v = static_cast<int64_t>(zz >> 1) ^ -static_cast<int64_t>(zz & 1);
            return true;
        }

        bool Bool(bool& v)
        {
            uint8_t b = 0;
            if (!U8(b)) return false;
            v = (b != 0);
            return true;
        }

        /// Строка: u16 длины + UTF-8 байты (как пишет Rust `Writer::str`).
        bool Str(std::string& v)
        {
            const uint8_t* p = nullptr;
            if (!Take(2, p)) return false;
            const uint16_t n = static_cast<uint16_t>(p[0]) | (static_cast<uint16_t>(p[1]) << 8);
            if (Pos + n > Len) return false;
            v.assign(reinterpret_cast<const char*>(Data + Pos), static_cast<size_t>(n));
            Pos += static_cast<size_t>(n);
            return true;
        }
    };

    /// Прочитанные заголовки пакета.
    struct PacketHeader
    {
        uint64_t Token = 0;
        uint32_t BodyLen = 0;
        size_t BodyOffset = 0;
    };

    /// Проверить заголовок пакета и токен сессии.
    inline bool ReadPacketHeader(const uint8_t* data, size_t len, uint64_t expected_token, PacketHeader& out)
    {
        if (len < 12) return false;
        uint64_t token = 0;
        for (int i = 0; i < 8; ++i)
        {
            token |= static_cast<uint64_t>(data[i]) << (8 * i);
        }
        if (token != expected_token) return false;
        uint32_t blen = static_cast<uint32_t>(data[8]) | (static_cast<uint32_t>(data[9]) << 8)
                      | (static_cast<uint32_t>(data[10]) << 16) | (static_cast<uint32_t>(data[11]) << 24);
        if (blen > MaxDatagram) return false;
        if (len < 12 + blen) return false;
        out.Token = token;
        out.BodyLen = blen;
        out.BodyOffset = 12;
        return true;
    }

    /// Разобрать снимок (tag 5). Позиции переводятся в сантиметры UE.
    inline bool DecodeSnapshot(const uint8_t* body, size_t len, Snapshot& out)    {
        Reader r{body, len, 0};
        uint8_t tag = 0;
        if (!r.U8(tag)) return false;
        if (tag != TagSnapshotV2) return false;

        if (!r.Varint(out.Tick)) return false;
        uint64_t ack = 0;
        if (!r.Varint(ack)) return false;
        out.AckInputSeq = static_cast<int32_t>(ack);
        uint8_t baseline = 0;
        if (!r.U8(baseline)) return false;
        out.Baseline = baseline != 0;

        uint64_t count = 0;
        if (!r.Varint(count)) return false;
        out.Entities.clear();
        out.Entities.reserve(static_cast<size_t>(count));

        for (uint64_t i = 0; i < count; ++i)
        {
            EntityState e;
            uint64_t id = 0;
            if (!r.Varint(id)) return false;
            e.Id = static_cast<int32_t>(id);

            int64_t px = 0, py = 0, pz = 0;
            if (!r.SVarint(px) || !r.SVarint(py) || !r.SVarint(pz)) return false;
            int64_t vx = 0, vy = 0, vz = 0;
            if (!r.SVarint(vx) || !r.SVarint(vy) || !r.SVarint(vz)) return false;
            if (!r.F32(e.Yaw) || !r.F32(e.Pitch)) return false;

            e.X = static_cast<float>(px * WireToCm);
            e.Y = static_cast<float>(py * WireToCm);
            e.Z = static_cast<float>(pz * WireToCm);
            e.VX = static_cast<float>(vx * WireToCm);
            e.VY = static_cast<float>(vy * WireToCm);
            e.VZ = static_cast<float>(vz * WireToCm);
            out.Entities.push_back(e);
        }
        return true;
    }

    /// Разобрать приветствие сервера (tag 8): id своей сущности.
    inline bool DecodeWelcome(const uint8_t* body, size_t len, int32_t& out_entity)
    {
        Reader r{body, len, 0};
        uint8_t tag = 0;
        if (!r.U8(tag) || tag != TagWelcome) return false;
        uint64_t id = 0;
        if (!r.Varint(id)) return false;
        out_entity = static_cast<int32_t>(id);
        return true;
    }

    /// Разобрать результат строительства (tag 7).
    inline bool DecodeBuildResult(const uint8_t* body, size_t len, BuildResult& out)
    {
        Reader r{body, len, 0};
        uint8_t tag = 0;
        if (!r.U8(tag) || tag != TagBuildResult) return false;
        uint8_t accepted = 0;
        if (!r.U8(accepted)) return false;
        out.Accepted = accepted != 0;
        uint8_t reason = 0;
        if (!r.U8(reason)) return false;
        out.Reason = static_cast<BuildReason>(reason);
        return true;
    }

    /// Разобрать правку конструкции (tag 9).
    ///
    /// Формат: entity(varint), author(varint), action(u8),
    ///         coord(3×i32 LE), color(3×u8), revision(varint).
    inline bool DecodeBuildEdit(const uint8_t* body, size_t len, BuildEdit& out)
    {
        Reader r{body, len, 0};
        uint8_t tag = 0;
        if (!r.U8(tag) || tag != TagBuildEdit) return false;
        uint64_t entity = 0, author = 0;
        if (!r.Varint(entity) || !r.Varint(author)) return false;
        out.Entity = static_cast<uint32_t>(entity);
        out.Author = static_cast<uint32_t>(author);
        uint8_t action = 0;
        if (!r.U8(action)) return false;
        out.Action = static_cast<BuildAction>(action);
        if (!r.I32(out.X) || !r.I32(out.Y) || !r.I32(out.Z)) return false;
        if (!r.U8(out.R) || !r.U8(out.G) || !r.U8(out.B)) return false;
        if (!r.Varint(out.Revision)) return false;
        return true;
    }

    /// Разобрать права на редактирование (tag 12).
    inline bool DecodeBuildAccess(const uint8_t* body, size_t len, BuildAccess& out)
    {
        Reader r{body, len, 0};
        uint8_t tag = 0;
        if (!r.U8(tag) || tag != TagBuildAccess) return false;
        uint64_t entity = 0;
        if (!r.Varint(entity)) return false;
        out.Entity = static_cast<uint32_t>(entity);
        uint8_t can = 0;
        if (!r.U8(can)) return false;
        out.CanEdit = can != 0;
        return true;
    }

    /// Разобрать событие выстрела (tag 13).
    inline bool DecodeShot(const uint8_t* body, size_t len, ShotEvent& out)
    {
        Reader r{body, len, 0};
        uint8_t tag = 0;
        if (!r.U8(tag) || tag != TagShot) return false;
        uint64_t attacker = 0, target = 0, damage = 0;
        if (!r.Varint(attacker) || !r.Varint(target)) return false;
        uint8_t hit = 0;
        if (!r.U8(hit)) return false;
        if (!r.Varint(damage)) return false;
        uint8_t destroyed = 0;
        if (!r.U8(destroyed)) return false;
        out.Attacker = static_cast<uint32_t>(attacker);
        out.Target = static_cast<uint32_t>(target);
        out.Hit = hit != 0;
        out.Damage = static_cast<uint32_t>(damage);
        out.Destroyed = destroyed != 0;
        return true;
    }

    /// Разобрать состояние корпуса (tag 14).
    inline bool DecodeHull(const uint8_t* body, size_t len, HullState& out)
    {
        Reader r{body, len, 0};
        uint8_t tag = 0;
        if (!r.U8(tag) || tag != TagHull) return false;
        uint64_t entity = 0, shield = 0, armor = 0;
        if (!r.Varint(entity) || !r.Varint(shield) || !r.Varint(armor)) return false;
        out.Entity = static_cast<uint32_t>(entity);
        out.Shield = static_cast<uint32_t>(shield);
        out.Armor = static_cast<uint32_t>(armor);
        uint8_t destroyed = 0;
        if (!r.U8(destroyed)) return false;
        out.Destroyed = destroyed != 0;
        return true;
    }

    /// Разобрать смерть (tag 15).
    inline bool DecodeDeath(const uint8_t* body, size_t len, DeathEvent& out)
    {
        Reader r{body, len, 0};
        uint8_t tag = 0;
        if (!r.U8(tag) || tag != TagDeath) return false;
        uint64_t entity = 0, tick = 0;
        if (!r.Varint(entity) || !r.Varint(tick)) return false;
        out.Entity = static_cast<uint32_t>(entity);
        out.RespawnTick = tick;
        return true;
    }

    /// Разобрать возрождение (tag 16).
    inline bool DecodeRespawn(const uint8_t* body, size_t len, RespawnEvent& out)
    {
        Reader r{body, len, 0};
        uint8_t tag = 0;
        if (!r.U8(tag) || tag != TagRespawn) return false;
        uint64_t entity = 0, until = 0;
        if (!r.Varint(entity) || !r.Varint(until)) return false;
        out.Entity = static_cast<uint32_t>(entity);
        out.InvulnUntil = until;
        return true;
    }

    /// Разобрать позу чужого персонажа (tag 18). Позиция — в сантиметрах UE.
    inline bool DecodeCharacter(const uint8_t* body, size_t len, CharacterPose& out)
    {
        Reader r{body, len, 0};
        uint8_t tag = 0;
        if (!r.U8(tag) || tag != TagServerCharacter) return false;
        uint64_t entity = 0;
        if (!r.Varint(entity)) return false;
        out.Entity = static_cast<uint32_t>(entity);
        int64_t px = 0, py = 0, pz = 0;
        if (!r.SVarint(px) || !r.SVarint(py) || !r.SVarint(pz)) return false;
        if (!r.F32(out.Yaw)) return false;
        uint8_t moving = 0;
        if (!r.U8(moving)) return false;
        out.Moving = moving != 0;
        out.X = static_cast<float>(px * WireToCm);
        out.Y = static_cast<float>(py * WireToCm);
        out.Z = static_cast<float>(pz * WireToCm);
        return true;
    }

    /// Разобрать описание системы (tag 19). Позиции — в серверных единицах.
    inline bool DecodeSystem(const uint8_t* body, size_t len, SystemInfo& out)
    {
        Reader r{body, len, 0};
        uint8_t tag = 0;
        if (!r.U8(tag) || tag != TagSystem) return false;
        uint64_t id = 0;
        if (!r.Varint(id)) return false;
        out.Id = static_cast<uint32_t>(id);
        // Строка: u16 длина + байты UTF-8.
        uint16_t nlen = 0;
        {
            const uint8_t* p = nullptr;
            if (!r.Take(2, p)) return false;
            nlen = static_cast<uint16_t>(p[0] | (p[1] << 8));
        }
        const uint8_t* nb = nullptr;
        if (!r.Take(nlen, nb)) return false;
        out.Name.assign(reinterpret_cast<const char*>(nb), nlen);

        uint64_t count = 0;
        if (!r.Varint(count)) return false;
        out.Bodies.clear();
        out.Bodies.reserve(static_cast<size_t>(count));
        for (uint64_t i = 0; i < count; ++i)
        {
            SystemBody b;
            uint64_t bid = 0;
            if (!r.Varint(bid)) return false;
            b.Id = static_cast<uint32_t>(bid);
            if (!r.U8(b.Kind)) return false;
            if (!r.U8(b.Surface)) return false;
            // Центр орбиты — f32 в серверных единицах (дальние тела не влезают
            // в штатное квантование i32 ±214 км).
            if (!r.F32(b.CenterX) || !r.F32(b.CenterY) || !r.F32(b.CenterZ)) return false;
            if (!r.F32(b.Orbit) || !r.F32(b.Phase) || !r.F32(b.Omega)) return false;
            // f64 LE — радиус в серверных единицах.
            const uint8_t* rb = nullptr;
            if (!r.Take(8, rb)) return false;
            uint64_t rbits = 0;
            for (int k = 0; k < 8; ++k) { rbits |= static_cast<uint64_t>(rb[k]) << (8 * k); }
            std::memcpy(&b.Radius, &rbits, 8);
            // f64 LE — амплитуда рельефа тела.
            const uint8_t* ab = nullptr;
            if (!r.Take(8, ab)) return false;
            uint64_t abits = 0;
            for (int k = 0; k < 8; ++k) { abits |= static_cast<uint64_t>(ab[k]) << (8 * k); }
            std::memcpy(&b.Amplitude, &abits, 8);
            // Сид рельефа (u64 LE).
            const uint8_t* sb = nullptr;
            if (!r.Take(8, sb)) return false;
            uint64_t sd = 0;
            for (int k = 0; k < 8; ++k) { sd |= static_cast<uint64_t>(sb[k]) << (8 * k); }
            b.Seed = sd;
            out.Bodies.push_back(b);
        }
        return true;
    }

    /// Собрать позу своего персонажа (клиент → сервер, тег 17).
    ///
    /// Позиция задаётся в сантиметрах UE (как в снимках), квантуется в провод.
    inline void push_varint(std::vector<uint8_t>& out, uint64_t v)
    {
        while (v >= 0x80)
        {
            out.push_back(static_cast<uint8_t>(v) | 0x80);
            v >>= 7;
        }
        out.push_back(static_cast<uint8_t>(v));
    }

    inline std::vector<uint8_t> EncodeCharacter(
        uint32_t entity, float xCm, float yCm, float zCm, float yaw, bool moving, bool active)
    {
        std::vector<uint8_t> out;
        out.push_back(TagClientCharacter);
        push_varint(out, entity);
        // Обратное квантование: см → провод (1/1000 единицы).
        auto push_svarint = [&out](int64_t v) {
            uint64_t zz = (static_cast<uint64_t>(v) << 1) ^ static_cast<uint64_t>(v >> 63);
            while (zz >= 0x80) { out.push_back(static_cast<uint8_t>(zz) | 0x80); zz >>= 7; }
            out.push_back(static_cast<uint8_t>(zz));
        };
        const double inv = 1.0 / (1.0 / 1000.0 * 100.0); // см → провод
        push_svarint(static_cast<int64_t>(std::llround(xCm * inv)));
        push_svarint(static_cast<int64_t>(std::llround(yCm * inv)));
        push_svarint(static_cast<int64_t>(std::llround(zCm * inv)));
        // yaw как f32 LE.
        uint32_t bits = 0;
        std::memcpy(&bits, &yaw, 4);
        out.push_back(bits & 0xFF);
        out.push_back((bits >> 8) & 0xFF);
        out.push_back((bits >> 16) & 0xFF);
        out.push_back((bits >> 24) & 0xFF);
        out.push_back(moving ? 1 : 0);
        out.push_back(active ? 1 : 0);
        return out;
    }

    // --- Кодирование (для тестов и клиента) ---

    /// Воронка, выкопанная игроком (общая с сервером). Зеркало `ou_protocol`.
    struct DentWire
    {
        uint32_t Slot = 0;
        double X = 0.0, Y = 0.0, Z = 0.0;
        double CosRadius = 1.0;
        double Depth = 0.0;
    };

    /// Собрать запрос копания (клиент → сервер, тег 20).    ///
    /// Формат: [tag][from_server=0][slot u32][dir f64×3][cos_radius f64][depth f64].
    inline std::vector<uint8_t> EncodeDig(
        uint32_t slot, double x, double y, double z, double cosRadius, double depth)
    {
        std::vector<uint8_t> out;
        out.push_back(TagDig);
        out.push_back(0);   // from_server = false
        auto push_u32 = [&out](uint32_t v) {
            out.push_back(static_cast<uint8_t>(v & 0xFF));
            out.push_back(static_cast<uint8_t>((v >> 8) & 0xFF));
            out.push_back(static_cast<uint8_t>((v >> 16) & 0xFF));
            out.push_back(static_cast<uint8_t>((v >> 24) & 0xFF));
        };
        auto push_f64 = [&out](double v) {
            uint64_t bits = 0;
            std::memcpy(&bits, &v, 8);
            for (int i = 0; i < 8; ++i) { out.push_back(static_cast<uint8_t>((bits >> (8 * i)) & 0xFF)); }
        };
        push_u32(slot);
        push_f64(x); push_f64(y); push_f64(z);
        push_f64(cosRadius);
        push_f64(depth);
        return out;
    }

    /// Разобрать воронку от сервера (тег 20, from_server=1).
    inline bool DecodeDig(const uint8_t* body, size_t len, DentWire& out)
    {
        Reader r{body, len, 0};
        uint8_t tag = 0;
        if (!r.U8(tag) || tag != TagDig) return false;
        uint8_t fromServer = 0;
        if (!r.U8(fromServer) || fromServer == 0) return false;
        const uint8_t* p = nullptr;
        if (!r.Take(4, p)) return false;
        out.Slot = static_cast<uint32_t>(p[0]) | (static_cast<uint32_t>(p[1]) << 8)
                 | (static_cast<uint32_t>(p[2]) << 16) | (static_cast<uint32_t>(p[3]) << 24);
        return r.F64(out.X) && r.F64(out.Y) && r.F64(out.Z)
            && r.F64(out.CosRadius) && r.F64(out.Depth);
    }

    /// Ордер на рынке (клиент → сервер, тег 21).
    inline std::vector<uint8_t> EncodeMarketOrder(uint8_t side, uint32_t item, uint64_t price, uint64_t qty)
    {
        std::vector<uint8_t> out;
        out.push_back(TagMarketOrder);
        out.push_back(side);
        push_varint(out, item);
        push_varint(out, price);
        push_varint(out, qty);
        return out;
    }

    /// Состояние экономики игрока (сервер → клиент, тег 22).
    struct MarketState
    {
        uint64_t Credits = 0;
        std::vector<std::pair<uint32_t, uint64_t>> Items;
        std::vector<std::tuple<uint32_t, uint64_t, uint64_t>> Bids;   // (item, price, qty)
        std::vector<std::tuple<uint32_t, uint64_t, uint64_t>> Asks;
    };

    inline bool DecodeMarketState(const uint8_t* body, size_t len, MarketState& out)
    {
        Reader r{body, len, 0};
        uint8_t tag = 0;
        if (!r.U8(tag) || tag != TagMarketState) return false;
        if (!r.Varint(out.Credits)) return false;
        uint64_t n = 0;
        if (!r.Varint(n)) return false;
        out.Items.clear();
        for (uint64_t i = 0; i < n; ++i)
        {
            uint64_t id = 0, q = 0;
            if (!r.Varint(id) || !r.Varint(q)) return false;
            out.Items.emplace_back(static_cast<uint32_t>(id), q);
        }
        auto read_levels = [&r](std::vector<std::tuple<uint32_t, uint64_t, uint64_t>>& dst) -> bool {
            uint64_t m = 0;
            if (!r.Varint(m)) return false;
            dst.clear();
            for (uint64_t i = 0; i < m; ++i)
            {
                uint64_t id = 0, price = 0, q = 0;
                if (!r.Varint(id) || !r.Varint(price) || !r.Varint(q)) return false;
                dst.emplace_back(static_cast<uint32_t>(id), price, q);
            }
            return true;
        };
        return read_levels(out.Bids) && read_levels(out.Asks);
    }

    /// Что добыто при копании (сервер → клиент, тег 23).
    struct DigYield
    {
        uint32_t Item = 0;
        uint64_t Qty = 0;
        uint8_t Layer = 0;      // 0 руда, 1 лёд, 2 кристалл, 3 металл
        double HoleDepth = 0.0; // глубина ямы (м)
    };

    inline bool DecodeDigYield(const uint8_t* body, size_t len, DigYield& out)
    {
        Reader r{body, len, 0};
        uint8_t tag = 0;
        if (!r.U8(tag) || tag != TagDigYield) return false;
        uint64_t item = 0, qty = 0;
        if (!r.Varint(item) || !r.Varint(qty)) return false;
        if (!r.U8(out.Layer)) return false;
        if (!r.F64(out.HoleDepth)) return false;
        out.Item = static_cast<uint32_t>(item);
        out.Qty = qty;
        return true;
    }

    /// Собрать запрос пинга (клиент → сервер, тег 24): [tag][nonce u32 LE].
    inline std::vector<uint8_t> EncodePing(uint32_t Nonce)
    {
        std::vector<uint8_t> out;
        out.push_back(TagPing);
        out.push_back(static_cast<uint8_t>(Nonce & 0xFF));
        out.push_back(static_cast<uint8_t>((Nonce >> 8) & 0xFF));
        out.push_back(static_cast<uint8_t>((Nonce >> 16) & 0xFF));
        out.push_back(static_cast<uint8_t>((Nonce >> 24) & 0xFF));
        return out;
    }

    /// Разобрать ответ пинга (сервер → клиент, тег 25): nonce эхом.
    inline bool DecodePong(const uint8_t* body, size_t len, uint32_t& OutNonce)
    {
        Reader r{body, len, 0};
        uint8_t tag = 0;
        if (!r.U8(tag) || tag != TagPong) return false;
        return r.U32(OutNonce);
    }

    /// Рецепт производства (для окна крафта).
    struct Recipe
    {
        uint32_t Id = 0;
        std::string Name;
        std::vector<std::pair<uint32_t, uint64_t>> Inputs;
        uint32_t OutItem = 0;
        uint64_t OutQty = 0;
        uint64_t Ticks = 0;
    };

    /// Разобрать каталог рецептов (сервер → клиент, тег 26).
    inline bool DecodeRecipes(const uint8_t* body, size_t len, std::vector<Recipe>& Out)
    {
        Reader r{body, len, 0};
        uint8_t tag = 0;
        if (!r.U8(tag) || tag != TagRecipes) return false;
        uint64_t n = 0;
        if (!r.Varint(n)) return false;
        Out.clear();
        Out.reserve(static_cast<size_t>(n));
        for (uint64_t k = 0; k < n; ++k)
        {
            Recipe Rc;
            uint64_t id = 0;
            if (!r.Varint(id)) return false;
            Rc.Id = static_cast<uint32_t>(id);
            if (!r.Str(Rc.Name)) return false;
            uint64_t ni = 0;
            if (!r.Varint(ni)) return false;
            for (uint64_t i = 0; i < ni; ++i)
            {
                uint64_t item = 0, qty = 0;
                if (!r.Varint(item) || !r.Varint(qty)) return false;
                Rc.Inputs.emplace_back(static_cast<uint32_t>(item), qty);
            }
            uint64_t oi = 0, oq = 0, tk = 0;
            if (!r.Varint(oi) || !r.Varint(oq) || !r.Varint(tk)) return false;
            Rc.OutItem = static_cast<uint32_t>(oi);
            Rc.OutQty = oq;
            Rc.Ticks = tk;
            Out.push_back(std::move(Rc));
        }
        return true;
    }

    /// Результат крафта (сервер → клиент, тег 28).
    struct CraftResult
    {
        bool Ok = false;
        uint32_t Item = 0;
        uint64_t Qty = 0;
        std::string Reason;
    };

    inline bool DecodeCraftResult(const uint8_t* body, size_t len, CraftResult& Out)
    {
        Reader r{body, len, 0};
        uint8_t tag = 0;
        if (!r.U8(tag) || tag != TagCraftResult) return false;
        if (!r.Bool(Out.Ok)) return false;
        uint64_t item = 0, qty = 0;
        if (!r.Varint(item) || !r.Varint(qty)) return false;
        Out.Item = static_cast<uint32_t>(item);
        Out.Qty = qty;
        return r.Str(Out.Reason);
    }

    /// Собрать запрос крафта (клиент → сервер, тег 27): [tag][recipe varint].
    inline std::vector<uint8_t> EncodeCraft(uint32_t Recipe)
    {
        std::vector<uint8_t> out;
        out.push_back(TagCraft);
        push_varint(out, Recipe);
        return out;
    }

    /// Территория (сервер → клиент, тег 29) для HUD и миникарты.
    struct Territory
    {
        uint32_t Id = 0;
        uint32_t Owner = 0;   // 0 — ничей, иначе id игрока-владельца
        uint8_t Status = 0;   // 0 мирная, 1 спорная, 2 укреплённая
        double X = 0.0, Y = 0.0, Z = 0.0;   // центр, серверные единицы
        double Radius = 0.0;
        uint32_t ControlShield = 0;
        uint32_t TaxPercent = 0;
        uint64_t CollectedTax = 0;
    };

    inline bool DecodeTerritory(const uint8_t* body, size_t len, std::vector<Territory>& Out)
    {
        Reader r{body, len, 0};
        uint8_t tag = 0;
        if (!r.U8(tag) || tag != TagTerritory) return false;
        uint64_t n = 0;
        if (!r.Varint(n)) return false;
        Out.clear();
        Out.reserve(static_cast<size_t>(n));
        for (uint64_t k = 0; k < n; ++k)
        {
            Territory T;
            uint64_t id = 0, owner = 0, shield = 0, tax = 0;
            if (!r.Varint(id) || !r.Varint(owner)) return false;
            T.Id = static_cast<uint32_t>(id);
            T.Owner = static_cast<uint32_t>(owner);
            if (!r.U8(T.Status)) return false;
            int64_t cx = 0, cy = 0, cz = 0;
            if (!r.SVarint(cx) || !r.SVarint(cy) || !r.SVarint(cz)) return false;
            const double Inv = 1.0 / 1000.0;   // POS_SCALE на проводе
            T.X = static_cast<double>(cx) * Inv;
            T.Y = static_cast<double>(cy) * Inv;
            T.Z = static_cast<double>(cz) * Inv;
            if (!r.F64(T.Radius)) return false;
            if (!r.Varint(shield) || !r.Varint(tax)) return false;
            T.ControlShield = static_cast<uint32_t>(shield);
            T.TaxPercent = static_cast<uint32_t>(tax);
            if (!r.Varint(T.CollectedTax)) return false;
            Out.push_back(T);
        }
        return true;
    }

    /// Собрать запрос захвата территории (клиент → сервер, тег 30): [tag][id varint].
    inline std::vector<uint8_t> EncodeCapture(uint32_t Id)
    {
        std::vector<uint8_t> out;
        out.push_back(TagCapture);
        push_varint(out, Id);
        return out;
    }

    /// Собрать запрос привязки скрипта-шаблона (клиент → сервер, тег 31).
    inline std::vector<uint8_t> EncodeSetScript(uint32_t Template)
    {
        std::vector<uint8_t> out;
        out.push_back(TagSetScript);
        push_varint(out, Template);
        return out;
    }

    /// Статус скрипта на объекте игрока (сервер → клиент, тег 32).
    struct ScriptStatus
    {
        uint32_t Entity = 0;
        uint32_t Template = 0;   // 0 — скрипта нет
        std::string Name;
        uint64_t Runs = 0;
        uint64_t Errors = 0;
        std::string Last;
        std::vector<std::string> Logs;
    };

    inline bool DecodeScriptStatus(const uint8_t* body, size_t len, ScriptStatus& Out)
    {
        Reader r{body, len, 0};
        uint8_t tag = 0;
        if (!r.U8(tag) || tag != TagScript) return false;
        uint64_t entity = 0, tpl = 0;
        if (!r.Varint(entity) || !r.Varint(tpl)) return false;
        Out.Entity = static_cast<uint32_t>(entity);
        Out.Template = static_cast<uint32_t>(tpl);
        if (!r.Str(Out.Name)) return false;
        if (!r.Varint(Out.Runs) || !r.Varint(Out.Errors)) return false;
        if (!r.Str(Out.Last)) return false;
        uint64_t n = 0;
        if (!r.Varint(n)) return false;
        Out.Logs.clear();
        Out.Logs.reserve(static_cast<size_t>(n));
        for (uint64_t i = 0; i < n; ++i)
        {
            std::string s;
            if (!r.Str(s)) return false;
            Out.Logs.push_back(std::move(s));
        }
        return true;
    }

    /// Шаблон скрипта из библиотеки (сервер → клиент, тег 33).
    struct ScriptTemplateInfo
    {
        uint32_t Id = 0;
        std::string Name;
        std::string Description;
    };

    inline bool DecodeScripts(const uint8_t* body, size_t len, std::vector<ScriptTemplateInfo>& Out)
    {
        Reader r{body, len, 0};
        uint8_t tag = 0;
        if (!r.U8(tag) || tag != TagScripts) return false;
        uint64_t n = 0;
        if (!r.Varint(n)) return false;
        Out.clear();
        Out.reserve(static_cast<size_t>(n));
        for (uint64_t i = 0; i < n; ++i)
        {
            ScriptTemplateInfo T;
            uint64_t id = 0;
            if (!r.Varint(id)) return false;
            T.Id = static_cast<uint32_t>(id);
            if (!r.Str(T.Name) || !r.Str(T.Description)) return false;
            Out.push_back(std::move(T));
        }
        return true;
    }

    /// Собрать запрос постановки шаблона постройки (клиент → сервер, тег 34).
    /// Формат: [tag][id varint][x i32][y i32][z i32][entity u32].
    inline std::vector<uint8_t> EncodePlaceBlueprint(uint32_t Id, int32_t X, int32_t Y, int32_t Z, uint32_t Entity)
    {
        std::vector<uint8_t> out;
        out.push_back(TagPlaceBlueprint);
        push_varint(out, Id);
        auto push_i32 = [&out](int32_t v) {
            const uint32_t u = static_cast<uint32_t>(v);
            for (int i = 0; i < 4; ++i) { out.push_back(static_cast<uint8_t>((u >> (8 * i)) & 0xFF)); }
        };
        push_i32(X); push_i32(Y); push_i32(Z);
        for (int i = 0; i < 4; ++i) { out.push_back(static_cast<uint8_t>((Entity >> (8 * i)) & 0xFF)); }
        return out;
    }

    /// Библиотека шаблонов постройки (сервер → клиент, тег 35).
    struct BlueprintInfo
    {
        uint32_t Id = 0;
        std::string Name;
        std::string Description;
    };

    inline bool DecodeBlueprints(const uint8_t* body, size_t len, std::vector<BlueprintInfo>& Out)
    {
        Reader r{body, len, 0};
        uint8_t tag = 0;
        if (!r.U8(tag) || tag != TagBlueprints) return false;
        uint64_t n = 0;
        if (!r.Varint(n)) return false;
        Out.clear();
        Out.reserve(static_cast<size_t>(n));
        for (uint64_t i = 0; i < n; ++i)
        {
            BlueprintInfo T;
            uint64_t id = 0;
            if (!r.Varint(id)) return false;
            T.Id = static_cast<uint32_t>(id);
            if (!r.Str(T.Name) || !r.Str(T.Description)) return false;
            Out.push_back(std::move(T));
        }
        return true;
    }

    /// Боевая статистика игрока (сервер → клиент, тег 36): убийства, урон, награда.
    struct CombatStats
    {
        uint64_t Kills = 0;
        uint64_t Damage = 0;
        uint64_t Bounty = 0;
    };

    inline bool DecodeCombat(const uint8_t* body, size_t len, CombatStats& Out)
    {
        Reader r{body, len, 0};
        uint8_t tag = 0;
        if (!r.U8(tag) || tag != TagCombat) return false;
        return r.Varint(Out.Kills) && r.Varint(Out.Damage) && r.Varint(Out.Bounty);
    }

    /// Опубликовать постройку в мастерской (клиент → сервер, тег 37).
    inline std::vector<uint8_t> EncodePublishBlueprint(const std::string& Name, uint64_t Price)
    {
        std::vector<uint8_t> out;
        out.push_back(TagPublishBlueprint);
        // Строка: u16 длина + UTF-8.
        out.push_back(static_cast<uint8_t>(Name.size() & 0xFF));
        out.push_back(static_cast<uint8_t>((Name.size() >> 8) & 0xFF));
        out.insert(out.end(), Name.begin(), Name.end());
        push_varint(out, Price);
        return out;
    }

    /// Купить шаблон (клиент → сервер, тег 38): [tag][id varint].
    inline std::vector<uint8_t> EncodeBuyBlueprint(uint32_t Id)
    {
        std::vector<uint8_t> out;
        out.push_back(TagBuyBlueprint);
        push_varint(out, Id);
        return out;
    }

    /// Поставить свой шаблон (клиент → сервер, тег 39): [tag][id][x][y][z][entity].
    inline std::vector<uint8_t> EncodePlaceOwnBlueprint(uint32_t Id, int32_t X, int32_t Y, int32_t Z, uint32_t Entity)
    {
        std::vector<uint8_t> out;
        out.push_back(TagPlaceOwnBlueprint);
        push_varint(out, Id);
        auto push_i32 = [&out](int32_t v) {
            const uint32_t u = static_cast<uint32_t>(v);
            for (int i = 0; i < 4; ++i) { out.push_back(static_cast<uint8_t>((u >> (8 * i)) & 0xFF)); }
        };
        push_i32(X); push_i32(Y); push_i32(Z);
        for (int i = 0; i < 4; ++i) { out.push_back(static_cast<uint8_t>((Entity >> (8 * i)) & 0xFF)); }
        return out;
    }

    /// Объявление мастерской и личная библиотека (сервер → клиент, тег 40).
    struct WorkshopListing
    {
        uint32_t Id = 0;
        std::string Name;
        uint32_t Author = 0;
        uint32_t Voxels = 0;
        uint64_t Price = 0;
    };
    struct OwnBlueprint
    {
        uint32_t Id = 0;
        std::string Name;
    };

    inline bool DecodeWorkshop(const uint8_t* body, size_t len,
        std::vector<WorkshopListing>& OutListings, std::vector<OwnBlueprint>& OutOwned)
    {
        Reader r{body, len, 0};
        uint8_t tag = 0;
        if (!r.U8(tag) || tag != TagWorkshop) return false;
        uint64_t nl = 0;
        if (!r.Varint(nl)) return false;
        OutListings.clear();
        OutListings.reserve(static_cast<size_t>(nl));
        for (uint64_t i = 0; i < nl; ++i)
        {
            WorkshopListing L;
            uint64_t id = 0, author = 0, voxels = 0;
            if (!r.Varint(id)) return false;
            L.Id = static_cast<uint32_t>(id);
            if (!r.Str(L.Name)) return false;
            if (!r.Varint(author) || !r.Varint(voxels)) return false;
            L.Author = static_cast<uint32_t>(author);
            L.Voxels = static_cast<uint32_t>(voxels);
            if (!r.Varint(L.Price)) return false;
            OutListings.push_back(std::move(L));
        }
        uint64_t no = 0;
        if (!r.Varint(no)) return false;
        OutOwned.clear();
        OutOwned.reserve(static_cast<size_t>(no));
        for (uint64_t i = 0; i < no; ++i)
        {
            OwnBlueprint O;
            uint64_t id = 0;
            if (!r.Varint(id)) return false;
            O.Id = static_cast<uint32_t>(id);
            if (!r.Str(O.Name)) return false;
            OutOwned.push_back(std::move(O));
        }
        return true;
    }

    /// Результат операции мастерской (сервер → клиент, тег 41).
    struct WorkshopResult
    {
        bool Ok = false;
        std::string Message;
    };

    inline bool DecodeWorkshopResult(const uint8_t* body, size_t len, WorkshopResult& Out)
    {
        Reader r{body, len, 0};
        uint8_t tag = 0;
        if (!r.U8(tag) || tag != TagWorkshopResult) return false;
        if (!r.Bool(Out.Ok)) return false;
        return r.Str(Out.Message);
    }

    /// Собрать кадр ввода (клиент → сервер, тег 2).
    /// Формат (должен совпадать с `ou_protocol`):
    /// [tag][count u16][seq u32][throttle i16][yaw i16][pitch i16][strafe i16][up i16][fire u8][boost u8].
    inline std::vector<uint8_t> EncodeInput(
        uint32_t seq, float throttle, float yaw, float pitch, float strafe, float up, bool fire,
        bool boost = false)
    {
        std::vector<uint8_t> out;
        out.push_back(TagInput);
        out.push_back(1); // count = 1
        out.push_back(0);
        out.push_back(static_cast<uint8_t>(seq & 0xFF));
        out.push_back(static_cast<uint8_t>((seq >> 8) & 0xFF));
        out.push_back(static_cast<uint8_t>((seq >> 16) & 0xFF));
        out.push_back(static_cast<uint8_t>((seq >> 24) & 0xFF));
        auto push_unit = [&out](float v) {
            const float c = v < -1.f ? -1.f : (v > 1.f ? 1.f : v);
            const int16_t q = static_cast<int16_t>(std::lround(c * 32767.f));
            out.push_back(static_cast<uint8_t>(static_cast<uint16_t>(q) & 0xFF));
            out.push_back(static_cast<uint8_t>((static_cast<uint16_t>(q) >> 8) & 0xFF));
        };
        push_unit(throttle);
        push_unit(yaw);
        push_unit(pitch);
        push_unit(strafe);
        push_unit(up);
        out.push_back(fire ? 1 : 0);
        out.push_back(boost ? 1 : 0);
        return out;
    }

    /// Разобрать кадр ввода (для стенда).
    struct InputFrameView
    {
        uint32_t Seq = 0;
        float Throttle = 0.f, Yaw = 0.f, Pitch = 0.f, Strafe = 0.f, Up = 0.f;
        bool Fire = false;
    };

    inline bool DecodeInput(const uint8_t* p, size_t n, InputFrameView& out)
    {
        size_t i = 0;
        if (n < 1 + 2 + 4 + 10 + 1 || p[0] != TagInput) return false;
        i = 1;
        const uint16_t count = static_cast<uint16_t>(p[i] | (p[i + 1] << 8));
        i += 2;
        if (count < 1) return false;
        out.Seq = static_cast<uint32_t>(p[i]) | (static_cast<uint32_t>(p[i + 1]) << 8)
            | (static_cast<uint32_t>(p[i + 2]) << 16) | (static_cast<uint32_t>(p[i + 3]) << 24);
        i += 4;
        auto read_unit = [&p, &i]() -> float {
            const int16_t q = static_cast<int16_t>(static_cast<uint16_t>(p[i]) | (static_cast<uint16_t>(p[i + 1]) << 8));
            i += 2;
            return static_cast<float>(q) / 32767.f;
        };
        out.Throttle = read_unit();
        out.Yaw = read_unit();
        out.Pitch = read_unit();
        out.Strafe = read_unit();
        out.Up = read_unit();
        out.Fire = p[i] != 0;
        return true;
    }

    /// Собрать команду строительства.
    inline std::vector<uint8_t> EncodeBuild(const BuildCommand& cmd)
    {
        std::vector<uint8_t> out;
        out.push_back(TagBuild);
        const uint32_t e = cmd.Entity;
        out.push_back(static_cast<uint8_t>(e & 0xFF));
        out.push_back(static_cast<uint8_t>((e >> 8) & 0xFF));
        out.push_back(static_cast<uint8_t>((e >> 16) & 0xFF));
        out.push_back(static_cast<uint8_t>((e >> 24) & 0xFF));
        out.push_back(static_cast<uint8_t>(cmd.Action));
        auto push_i32 = [&out](int32_t v) {
            const uint32_t u = static_cast<uint32_t>(v);
            out.push_back(static_cast<uint8_t>(u & 0xFF));
            out.push_back(static_cast<uint8_t>((u >> 8) & 0xFF));
            out.push_back(static_cast<uint8_t>((u >> 16) & 0xFF));
            out.push_back(static_cast<uint8_t>((u >> 24) & 0xFF));
        };
        push_i32(cmd.X);
        push_i32(cmd.Y);
        push_i32(cmd.Z);
        out.push_back(cmd.R);
        out.push_back(cmd.G);
        out.push_back(cmd.B);
        return out;
    }

    /// Собрать подтверждение правки (клиент → сервер, тег 10).
    inline std::vector<uint8_t> EncodeBuildAck(uint32_t entity, uint64_t revision)
    {
        std::vector<uint8_t> out;
        out.push_back(TagBuildAck);
        push_varint(out, entity);
        push_varint(out, revision);
        return out;
    }

    /// Обернуть тело сообщения в пакет с токеном.
    inline std::vector<uint8_t> EncodePacket(uint64_t token, const std::vector<uint8_t>& body)
    {
        std::vector<uint8_t> pkt;
        pkt.reserve(12 + body.size());
        for (int i = 0; i < 8; ++i)
        {
            pkt.push_back(static_cast<uint8_t>((token >> (8 * i)) & 0xFF));
        }
        const uint32_t len = static_cast<uint32_t>(body.size());
        for (int i = 0; i < 4; ++i)
        {
            pkt.push_back(static_cast<uint8_t>((len >> (8 * i)) & 0xFF));
        }
        pkt.insert(pkt.end(), body.begin(), body.end());
        return pkt;
    }
}
