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

    std::printf("\n=== Итог ===\n");
    if (g_failures == 0)
    {
        std::printf("Все проверки пройдены: C++-клиент корректно читает протокол Rust-сервера.\n");
        return 0;
    }
    std::printf("Провалено проверок: %d\n", g_failures);
    return 1;
}
