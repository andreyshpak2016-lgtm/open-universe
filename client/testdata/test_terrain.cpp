// Кросс-проверка рельефа: C++-формула клиента против эталона Rust-сервера.
//
// Зачем: высота поверхности — общие данные мира. Если формулы разойдутся,
// картинка на клиенте не совпадёт с физикой (игрок «утонет» или повиснет).
//
// Стенд читает файл-эталон (его печатает `ou-terrain-ref`) и для каждого
// направления считает высоту той же общей формулой, что использует игра
// (`OUTerrain.h`), после чего сверяет с эталоном.
//
// Запуск (см. tools/test-terrain.ps1):
//   test_terrain.exe terrain_ref.txt

#define _CRT_SECURE_NO_WARNINGS
#include "OUTerrain.h"

#include <cmath>
#include <cstdio>
#include <cstdint>
#include <cstring>
#include <string>
#include <vector>

namespace
{
    int g_failures = 0;

    void Check(bool cond, const std::string& label)
    {
        if (cond)
        {
            std::printf("  [OK]   %s\n", label.c_str());
        }
        else
        {
            std::printf("  [FAIL] %s\n", label.c_str());
            ++g_failures;
        }
    }
}

int main(int argc, char** argv)
{
    const std::string path = (argc > 1) ? argv[1] : "terrain_ref.txt";
    std::FILE* f = std::fopen(path.c_str(), "rb");
    if (!f)
    {
        std::printf("не открыть эталон: %s\n", path.c_str());
        return 2;
    }

    std::printf("Open Universe — кросс-проверка рельефа (C++ против Rust)\n");
    std::printf("Эталон: %s\n\n", path.c_str());

    int version = 0;
    double radius = 0.0, amplitude = 0.0, seaLevel = 0.0;
    uint64_t seed = 0;
    double spawnX = 0.0, spawnY = 0.0, spawnZ = 0.0;
    bool haveSpawn = false;

    struct Row { double x, y, z, h0, h1; int biome; };
    std::vector<Row> rows;

    char line[512];
    while (std::fgets(line, sizeof(line), f))
    {
        double x, y, z, h0, h1;
        int biome;
        if (std::sscanf(line, "version %d", &version) == 1) continue;
        if (std::sscanf(line, "radius %lf", &radius) == 1) continue;
        if (std::sscanf(line, "amplitude %lf", &amplitude) == 1) continue;
        if (std::sscanf(line, "sea_level %lf", &seaLevel) == 1) continue;
        if (std::sscanf(line, "seed %llu", reinterpret_cast<unsigned long long*>(&seed)) == 1) continue;
        if (std::sscanf(line, "spawn %lf %lf %lf", &spawnX, &spawnY, &spawnZ) == 3) { haveSpawn = true; continue; }
        if (std::sscanf(line, "height %lf %lf %lf %lf %lf %d", &x, &y, &z, &h0, &h1, &biome) == 6)
        {
            rows.push_back({ x, y, z, h0, h1, biome });
        }
    }
    std::fclose(f);

    std::printf("=== 1. Параметры рельефа ===\n");
    char buf[256];
    std::snprintf(buf, sizeof(buf), "версия формулы: клиент %d == сервер %d",
        OULT::TerrainVersion, version);
    Check(OULT::TerrainVersion == version, buf);
    std::snprintf(buf, sizeof(buf), "радиус совпал (%.0f)", radius);
    Check(std::fabs(radius - 500000.0) < 1.0, buf);
    std::snprintf(buf, sizeof(buf), "амплитуда совпала (%.0f)", amplitude);
    Check(std::fabs(amplitude - 10000.0) < 1.0, buf);

    std::printf("\n=== 2. Высоты (общая формула) ===\n");
    std::printf("  направлений: %zu\n", rows.size());

    double maxErr0 = 0.0;
    double maxErr1 = 0.0;
    int mismatches = 0;
    int biomeMismatch = 0;

    for (const Row& r : rows)
    {
        // Крупный уровень (глобальный меш) и полный (физика/локальный участок).
        const double c0 = OULT::HeightDir(r.x, r.y, r.z, amplitude, seaLevel, seed, 0);
        const double c1 = OULT::HeightDir(r.x, r.y, r.z, amplitude, seaLevel, seed, 1);
        const double err0 = std::fabs(c0 - r.h0);
        const double err1 = std::fabs(c1 - r.h1);
        if (err0 > maxErr0) maxErr0 = err0;
        if (err1 > maxErr1) maxErr1 = err1;
        if (err0 > 1e-6 || err1 > 1e-6)
        {
            ++mismatches;
            if (mismatches <= 3)
            {
                std::printf("  [!] dir (%.3f,%.3f,%.3f): detail0 клиент %.10f сервер %.10f (%.3e);"
                    " detail1 клиент %.10f сервер %.10f (%.3e)\n",
                    r.x, r.y, r.z, c0, r.h0, err0, c1, r.h1, err1);
            }
        }
        // Биом считается по ПОЛНОМУ рельефу (как на сервере).
        const int b = static_cast<int>(OULT::BiomeFromHeight(c1, amplitude));
        if (b != r.biome) ++biomeMismatch;
    }

    std::snprintf(buf, sizeof(buf), "крупный рельеф совпал бит-в-бит (макс. разница %.2e)", maxErr0);
    Check(maxErr0 < 1e-6, buf);
    std::snprintf(buf, sizeof(buf), "полный рельеф совпал бит-в-бит (макс. разница %.2e)", maxErr1);
    Check(maxErr1 < 1e-6, buf);
    std::snprintf(buf, sizeof(buf), "биомы совпали (расхождений %d)", biomeMismatch);
    Check(biomeMismatch == 0, buf);

    std::printf("\n=== 3. Точка появления (самое ровное место) ===\n");
    {
        double sx = 0.0, sy = 1.0, sz = 0.0;
        OULT::HomeSpawnDirectionServer(amplitude, seaLevel, seed, sx, sy, sz);
        const double dx = sx - spawnX, dy = sy - spawnY, dz = sz - spawnZ;
        const double err = std::sqrt(dx * dx + dy * dy + dz * dz);
        std::snprintf(buf, sizeof(buf), "точка появления совпала (макс. разница %.2e)", err);
        Check(haveSpawn && err < 1e-9, buf);
        // Площадка под базу должна быть ровной.
        const double rough = OULT::SpawnRoughness(sx, sy, sz, OULT::SpawnFlatRadiusM, amplitude, seaLevel, seed);
        std::snprintf(buf, sizeof(buf), "площадка под базу ровная (перепад %.1f м на %.0f м)",
            rough, OULT::SpawnFlatRadiusM);
        Check(rough < 40.0, buf);
    }

    std::printf("\n=== Итог ===\n");
    if (g_failures == 0)
    {
        std::printf("Рельеф клиента совпадает с рельефом сервера.\n");
        return 0;
    }
    std::printf("Провалено проверок: %d\n", g_failures);
    return 1;
}
