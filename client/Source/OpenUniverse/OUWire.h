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

    /// Масштаб позиции: 1 единица провода = 1e-4 игровых единиц.
    constexpr double WireScale = 1.0 / 10000.0;
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

        bool F32(float& v)
        {
            const uint8_t* p = nullptr;
            if (!Take(4, p)) return false;
            uint32_t bits = static_cast<uint32_t>(p[0]) | (static_cast<uint32_t>(p[1]) << 8)
                          | (static_cast<uint32_t>(p[2]) << 16) | (static_cast<uint32_t>(p[3]) << 24);
            std::memcpy(&v, &bits, 4);
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
    inline bool DecodeSnapshot(const uint8_t* body, size_t len, Snapshot& out)
    {
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

    // --- Кодирование (для тестов и клиента) ---

    /// Дописать LEB128-varint в буфер.
    inline void push_varint(std::vector<uint8_t>& out, uint64_t v)
    {
        while (v >= 0x80)
        {
            out.push_back(static_cast<uint8_t>(v) | 0x80);
            v >>= 7;
        }
        out.push_back(static_cast<uint8_t>(v));
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
