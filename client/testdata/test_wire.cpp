// Тестовый стенд клиентского протокола Open Universe.
//
// Компилируется обычным C++ (без Unreal) и проверяет, что наш клиентский
// разбор `OUWire.h` корректно декодирует **реальный пакет Rust-сервера**
// (`client/testdata/packet.dat`), созданный `ou-dump-packet`.
//
// Запуск (см. tools/test-client.ps1):
//   cl /std:c++20 /EHsc test_wire.cpp && test_wire.exe packet.dat

#include "OUWire.h"

#include <cstdio>
#include <cmath>
#include <vector>
#include <string>

static int g_failures = 0;

static void Check(bool ok, const char* what)
{
    if (ok)
    {
        std::printf("  [OK]   %s\n", what);
    }
    else
    {
        std::printf("  [FAIL] %s\n", what);
        ++g_failures;
    }
}

static std::vector<uint8_t> ReadFile(const std::string& path)
{
    std::vector<uint8_t> data;
    std::FILE* f = std::fopen(path.c_str(), "rb");
    if (!f) return data;
    std::fseek(f, 0, SEEK_END);
    const long size = std::ftell(f);
    std::fseek(f, 0, SEEK_SET);
    data.resize(static_cast<size_t>(size));
    if (size > 0)
    {
        std::fread(data.data(), 1, static_cast<size_t>(size), f);
    }
    std::fclose(f);
    return data;
}

/// Эталонные значения из `ou-dump-packet` (допуск — 1 см).
struct Expected
{
    int32_t Id;
    float X, Y, Z;
    float Yaw;
};

int main(int argc, char** argv)
{
    const std::string path = argc > 1 ? argv[1] : "packet.dat";
    const uint64_t token = 0x376085675394cb58ull;

    std::printf("Open Universe — проверка клиентского протокола (C++)\n");
    std::printf("Пакет: %s\n\n", path.c_str());

    const std::vector<uint8_t> packet = ReadFile(path);
    Check(!packet.empty(), "пакет прочитан с диска");
    if (packet.empty())
    {
        return 1;
    }
    std::printf("  размер пакета: %zu байт\n", packet.size());

    // --- 1. Заголовок и токен ---
    std::printf("\n=== 1. Заголовок и токен сессии ===\n");
    OUWire::PacketHeader hdr;
    Check(OUWire::ReadPacketHeader(packet.data(), packet.size(), token, hdr),
          "заголовок прочитан, токен совпал");
    Check(!OUWire::ReadPacketHeader(packet.data(), packet.size(), 12345ull, hdr),
          "чужой токен отклонён");
    std::printf("  длина тела: %u байт\n", hdr.BodyLen);

    // --- 2. Разбор снимка ---
    std::printf("\n=== 2. Разбор снимка (snapshot v2) ===\n");
    const uint8_t* body = packet.data() + hdr.BodyOffset;
    OUWire::Snapshot snap;
    Check(OUWire::DecodeSnapshot(body, hdr.BodyLen, snap), "снимок разобран");
    std::printf("  тик: %llu, baseline: %s, сущностей: %zu\n",
                static_cast<unsigned long long>(snap.Tick),
                snap.Baseline ? "да" : "нет",
                snap.Entities.size());
    Check(snap.Entities.size() == 3, "в снимке три сущности");
    Check(snap.Tick == 61, "номер тика совпал с сервером");

    // --- 3. Сверка значений с эталоном ---
    std::printf("\n=== 3. Сверка позиций с сервером (допуск 1 см) ===\n");
    const Expected expected[3] = {
        {1,  1169.48f,  327.47f,  9212.51f,  0.3660f},
        {2,  9709.25f,  327.47f,   890.97f, -0.7320f},
        {3, -5607.76f, 3327.47f, 24309.50f,  0.1220f},
    };

    for (int i = 0; i < 3; ++i)
    {
        const OUWire::EntityState* e = nullptr;
        for (const auto& s : snap.Entities)
        {
            if (s.Id == expected[i].Id) e = &s;
        }
        if (!e)
        {
            std::printf("  [FAIL] сущность id %d не найдена\n", expected[i].Id);
            ++g_failures;
            continue;
        }
        const float dx = std::fabs(e->X - expected[i].X);
        const float dy = std::fabs(e->Y - expected[i].Y);
        const float dz = std::fabs(e->Z - expected[i].Z);
        const float dyaw = std::fabs(e->Yaw - expected[i].Yaw);
        const bool pos_ok = dx < 1.0f && dy < 1.0f && dz < 1.0f;
        const bool yaw_ok = dyaw < 1e-4f;

        char msg[160];
        std::snprintf(msg, sizeof(msg),
                      "id %d: pos (%.2f, %.2f, %.2f), yaw %.4f",
                      e->Id, e->X, e->Y, e->Z, e->Yaw);
        Check(pos_ok && yaw_ok, msg);

        if (!pos_ok)
        {
            std::printf("         ожидалось (%.2f, %.2f, %.2f), разница %.2f/%.2f/%.2f\n",
                        expected[i].X, expected[i].Y, expected[i].Z, dx, dy, dz);
        }
    }

    // --- 4. Команда строительства: кодирование и разбор ---
    std::printf("\n=== 4. Команда строительства (клиент → сервер) ===\n");
    OUWire::BuildCommand cmd;
    cmd.Entity = 7;
    cmd.Action = OUWire::BuildPaint;
    cmd.X = -3; cmd.Y = 0; cmd.Z = 12;
    cmd.R = 10; cmd.G = 20; cmd.B = 30;

    const std::vector<uint8_t> body_out = OUWire::EncodeBuild(cmd);
    const std::vector<uint8_t> pkt_out = OUWire::EncodePacket(token, body_out);

    OUWire::PacketHeader h2;
    Check(OUWire::ReadPacketHeader(pkt_out.data(), pkt_out.size(), token, h2),
          "собранный пакет читается обратно");
    Check(body_out[0] == OUWire::TagBuild, "тег команды строительства = 6");

    // Разбираем вручную (серверная сторона формата).
    OUWire::Reader r{body_out.data(), body_out.size(), 1};
    uint32_t entity = 0;
    {
        const uint8_t* p = nullptr;
        r.Take(4, p);
        entity = static_cast<uint32_t>(p[0]) | (static_cast<uint32_t>(p[1]) << 8)
               | (static_cast<uint32_t>(p[2]) << 16) | (static_cast<uint32_t>(p[3]) << 24);
    }
    uint8_t action = 0;
    r.U8(action);
    int32_t x = 0, y = 0, z = 0;
    r.I32(x); r.I32(y); r.I32(z);
    Check(entity == 7 && action == OUWire::BuildPaint && x == -3 && y == 0 && z == 12,
          "поля команды строительства сохранены");

    // --- 5. Устойчивость к повреждению ---
    std::printf("\n=== 5. Устойчивость к повреждённому пакету ===\n");
    {
        std::vector<uint8_t> broken = packet;
        broken.resize(broken.size() / 2); // обрезали
        OUWire::PacketHeader h3;
        const bool hdr_ok = OUWire::ReadPacketHeader(broken.data(), broken.size(), token, h3);
        const bool decoded = hdr_ok && OUWire::DecodeSnapshot(broken.data() + 12, h3.BodyLen, snap);
        Check(!decoded, "обрезанный пакет не разобран (без падения)");
    }
    {
        std::vector<uint8_t> corrupted = packet;
        for (size_t i = 12; i < corrupted.size(); ++i) corrupted[i] = 0xFF;
        OUWire::Snapshot s2;
        const bool ok = OUWire::DecodeSnapshot(corrupted.data() + 12, corrupted.size() - 12, s2);
        Check(!ok || s2.Entities.size() < 1000, "мусорный пакет не приводит к лавине данных");
    }

    // --- 6. Результат строительства ---
    std::printf("\n=== 6. Результат строительства (сервер → клиент) ===\n");
    {
        // Собираем ответ [tag=7][accepted=1][reason=0].
        const uint8_t resp[3] = {OUWire::TagBuildResult, 1, OUWire::BuildOk};
        OUWire::BuildResult br;
        Check(OUWire::DecodeBuildResult(resp, 3, br), "ответ разобран");
        Check(br.Accepted && br.Reason == OUWire::BuildOk, "команда принята сервером");
    }

    // --- 6a. Кадр ввода (клиент → сервер, тег 2) ---
    std::printf("\n=== 6a. Кадр управления (клиент → сервер) ===\n");
    {
        const std::vector<uint8_t> ib =
            OUWire::EncodeInput(42, 1.0f, -0.5f, 0.25f, -0.75f, 1.0f, true);
        Check(ib[0] == OUWire::TagInput, "тег кадра управления = 2");
        OUWire::InputFrameView iv;
        Check(OUWire::DecodeInput(ib.data(), ib.size(), iv), "кадр управления разобран");
        Check(iv.Seq == 42, "seq совпал");
        Check(std::fabs(iv.Throttle - 1.0f) < 1e-3f, "тяга совпала");
        Check(std::fabs(iv.Yaw + 0.5f) < 1e-3f, "рыскание совпало");
        Check(std::fabs(iv.Pitch - 0.25f) < 1e-3f, "тангаж совпал");
        Check(std::fabs(iv.Strafe + 0.75f) < 1e-3f, "стрейф совпал");
        Check(std::fabs(iv.Up - 1.0f) < 1e-3f, "подъём совпал");
        Check(iv.Fire, "огонь совпал");
    }

    // --- 7. Приветствие сервера (какой сущностью управляем) ---
    std::printf("\n=== 7. Приветствие сервера (welcome) ===\n");
    {
        // [tag=8][entity varint]. Для 300 varint = 0xAC 0x02.
        const uint8_t w1[2] = {OUWire::TagWelcome, 7};
        int32_t entity = -1;
        Check(OUWire::DecodeWelcome(w1, 2, entity) && entity == 7, "entity=7 разобран");

        const uint8_t w2[3] = {OUWire::TagWelcome, 0xAC, 0x02}; // 300
        Check(OUWire::DecodeWelcome(w2, 3, entity) && entity == 300, "entity=300 (varint) разобран");

        const uint8_t wbad[1] = {OUWire::TagSnapshotV2};
        Check(!OUWire::DecodeWelcome(wbad, 1, entity), "чужой тег отклонён");
    }

    // --- 8. Правка конструкции (сервер → клиент, авторитетное строительство) ---
    std::printf("\n=== 8. Правка конструкции (build_edit, tag 9) ===\n");
    {
        // Формат: entity(varint), author(varint), action(u8),
        //         coord(3×i32 LE), color(3×u8), revision(varint).
        std::vector<uint8_t> b;
        b.push_back(OUWire::TagBuildEdit);
        OUWire::push_varint(b, 3);      // entity
        OUWire::push_varint(b, 5);      // author
        b.push_back(OUWire::BuildPaint);
        auto push_i32 = [&b](int32_t v) {
            const uint32_t u = static_cast<uint32_t>(v);
            b.push_back(u & 0xFF);
            b.push_back((u >> 8) & 0xFF);
            b.push_back((u >> 16) & 0xFF);
            b.push_back((u >> 24) & 0xFF);
        };
        push_i32(-4); push_i32(7); push_i32(0);
        b.push_back(11); b.push_back(22); b.push_back(33);
        OUWire::push_varint(b, 128);

        OUWire::BuildEdit e;
        Check(OUWire::DecodeBuildEdit(b.data(), b.size(), e), "правка конструкции разобрана");
        Check(e.Entity == 3 && e.Author == 5 && e.Action == OUWire::BuildPaint,
              "entity/author/action совпали");
        Check(e.X == -4 && e.Y == 7 && e.Z == 0, "координаты вокселя совпали");
        Check(e.R == 11 && e.G == 22 && e.B == 33, "цвет совпал");
        Check(e.Revision == 128, "ревизия (varint) совпала");

        // Права на редактирование (tag 12).
        std::vector<uint8_t> ac;
        ac.push_back(OUWire::TagBuildAccess);
        OUWire::push_varint(ac, 4);
        ac.push_back(1);
        OUWire::BuildAccess acc;
        Check(OUWire::DecodeBuildAccess(ac.data(), ac.size(), acc) && acc.Entity == 4 && acc.CanEdit,
              "права на конструкцию разобраны (can_edit=1)");

        // Подтверждение правки (тег 10) кодируется и оборачивается.
        const std::vector<uint8_t> ack = OUWire::EncodeBuildAck(2, 300);
        Check(ack[0] == OUWire::TagBuildAck, "тег подтверждения = 10");
        OUWire::Reader ra{ack.data(), ack.size(), 1};
        uint64_t ack_entity = 0, ack_rev = 0;
        Check(ra.Varint(ack_entity) && ra.Varint(ack_rev) && ack_entity == 2 && ack_rev == 300,
              "поля подтверждения сохранены");
    }

    // --- 9. Бой: событие выстрела и состояние корпуса (теги 13, 14) ---
    std::printf("\n=== 9. Бой: выстрел и корпус (теги 13, 14) ===\n");
    {
        // Выстрел: [tag][attacker varint][target varint][hit u8][damage varint][destroyed u8]
        std::vector<uint8_t> sb;
        sb.push_back(OUWire::TagShot);
        OUWire::push_varint(sb, 1);    // attacker
        OUWire::push_varint(sb, 2);    // target
        sb.push_back(1);               // hit
        OUWire::push_varint(sb, 42);   // damage
        sb.push_back(0);               // destroyed
        OUWire::ShotEvent se;
        Check(OUWire::DecodeShot(sb.data(), sb.size(), se), "событие выстрела разобрано");
        Check(se.Attacker == 1 && se.Target == 2 && se.Hit && se.Damage == 42 && !se.Destroyed,
              "поля выстрела совпали");

        // Промах без цели.
        std::vector<uint8_t> mb;
        mb.push_back(OUWire::TagShot);
        OUWire::push_varint(mb, 3);
        OUWire::push_varint(mb, 0);
        mb.push_back(0);
        OUWire::push_varint(mb, 0);
        mb.push_back(0);
        OUWire::ShotEvent me;
        Check(OUWire::DecodeShot(mb.data(), mb.size(), me) && me.Target == 0 && !me.Hit,
              "промах без цели разобран");

        // Корпус: [tag][entity varint][shield varint][armor varint][destroyed u8]
        std::vector<uint8_t> hb;
        hb.push_back(OUWire::TagHull);
        OUWire::push_varint(hb, 7);
        OUWire::push_varint(hb, 30);
        OUWire::push_varint(hb, 80);
        hb.push_back(0);
        OUWire::HullState hs;
        Check(OUWire::DecodeHull(hb.data(), hb.size(), hs) && hs.Entity == 7 &&
              hs.Shield == 30 && hs.Armor == 80 && !hs.Destroyed,
              "состояние корпуса разобрано");

        // Смерть (tag 15) и возрождение (tag 16).
        std::vector<uint8_t> db;
        db.push_back(OUWire::TagDeath);
        OUWire::push_varint(db, 5);
        OUWire::push_varint(db, 4200);
        OUWire::DeathEvent de;
        Check(OUWire::DecodeDeath(db.data(), db.size(), de) && de.Entity == 5 && de.RespawnTick == 4200,
              "смерть разобрана (respawn_tick)");

        std::vector<uint8_t> rb;
        rb.push_back(OUWire::TagRespawn);
        OUWire::push_varint(rb, 5);
        OUWire::push_varint(rb, 4290);
        OUWire::RespawnEvent re;
        Check(OUWire::DecodeRespawn(rb.data(), rb.size(), re) && re.Entity == 5 && re.InvulnUntil == 4290,
              "возрождение разобрано (invuln_until)");

        // Поза персонажа (тег 17 кодируется, тег 18 разбирается).
        const std::vector<uint8_t> cb = OUWire::EncodeCharacter(2, 125.0f, -30.0f, 400.0f, 1.25f, true, true);
        Check(cb[0] == OUWire::TagClientCharacter, "тег позы персонажа = 17");

        std::vector<uint8_t> scb;
        scb.push_back(OUWire::TagServerCharacter);
        OUWire::push_varint(scb, 2);
        // Позиция 125/-30/400 см: обратное квантование (см → провод = см × 10
        // при масштабе 1 мм: 0.1 см на единицу провода).
        auto push_svarint = [&scb](int64_t v) {
            uint64_t zz = (static_cast<uint64_t>(v) << 1) ^ static_cast<uint64_t>(v >> 63);
            while (zz >= 0x80) { scb.push_back(static_cast<uint8_t>(zz) | 0x80); zz >>= 7; }
            scb.push_back(static_cast<uint8_t>(zz));
        };
        push_svarint(1250); push_svarint(-300); push_svarint(4000);
        const float yaw = 1.25f;
        uint32_t yb = 0; std::memcpy(&yb, &yaw, 4);
        scb.push_back(yb & 0xFF); scb.push_back((yb >> 8) & 0xFF);
        scb.push_back((yb >> 16) & 0xFF); scb.push_back((yb >> 24) & 0xFF);
        scb.push_back(1);
        OUWire::CharacterPose cp;
        Check(OUWire::DecodeCharacter(scb.data(), scb.size(), cp), "поза персонажа разобрана");
        Check(cp.Entity == 2 && std::fabs(cp.X - 125.0f) < 0.01f && std::fabs(cp.Z - 400.0f) < 0.01f && cp.Moving,
              "поля позы персонажа совпали");

        // Система (тег 19): имя, тела с типом поверхности и параметрами орбиты.
        std::vector<uint8_t> sysb;
        sysb.push_back(OUWire::TagSystem);
        OUWire::push_varint(sysb, 7);           // id
        const char* nm = "Alra-7";
        const uint16_t nlen = 6;
        sysb.push_back(nlen & 0xFF); sysb.push_back((nlen >> 8) & 0xFF);
        for (int k = 0; k < nlen; ++k) sysb.push_back(nm[k]);
        OUWire::push_varint(sysb, 1);           // одно тело
        OUWire::push_varint(sysb, 1);           // body id
        sysb.push_back(1);                      // kind=планета
        sysb.push_back(2);                      // surface=ледяная
        auto push_f32 = [&sysb](float f) {
            uint32_t bits = 0; std::memcpy(&bits, &f, 4);
            sysb.push_back(bits & 0xFF); sysb.push_back((bits >> 8) & 0xFF);
            sysb.push_back((bits >> 16) & 0xFF); sysb.push_back((bits >> 24) & 0xFF);
        };
        push_f32(25.0f); push_f32(0.0f); push_f32(0.0f);  // center (серверные ед.)
        push_f32(2500.0f);                                // orbit
        push_f32(0.5f);                                   // phase
        push_f32(0.02f);                                  // omega
        double rad = 120.0;
        uint64_t radBits = 0; std::memcpy(&radBits, &rad, 8);
        for (int k = 0; k < 8; ++k) sysb.push_back((radBits >> (8 * k)) & 0xFF);
        OUWire::SystemInfo si;
        Check(OUWire::DecodeSystem(sysb.data(), sysb.size(), si), "система разобрана");
        Check(si.Id == 7 && si.Name == "Alra-7" && si.Bodies.size() == 1, "id/имя/число тел совпали");
        Check(si.Bodies.size() == 1 && si.Bodies[0].Kind == 1 && si.Bodies[0].Surface == 2 &&
              std::fabs(si.Bodies[0].CenterX - 25.0f) < 0.01f &&
              std::fabs(si.Bodies[0].Orbit - 2500.0f) < 0.01f &&
              std::fabs(si.Bodies[0].Phase - 0.5f) < 1e-6f &&
              std::fabs(si.Bodies[0].Omega - 0.02f) < 1e-6f &&
              std::fabs(si.Bodies[0].Radius - 120.0) < 1e-9,
              "тело системы совпало (тип поверхности + параметры орбиты, f64 радиус)");
    }

    std::printf("\n=== Итог ===\n");
    if (g_failures == 0)
    {
        std::printf("Все проверки пройдены: C++-клиент корректно читает протокол Rust-сервера.\n");
        return 0;
    }
    std::printf("Провалено проверок: %d\n", g_failures);
    return 1;
}
