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

    struct Row { double x, y, z, h; int biome; };
    std::vector<Row> rows;

    char line[512];
    while (std::fgets(line, sizeof(line), f))
    {
        double x, y, z, h;
        int biome;
        if (std::sscanf(line, "version %d", &version) == 1) continue;
        if (std::sscanf(line, "radius %lf", &radius) == 1) continue;
        if (std::sscanf(line, "amplitude %lf", &amplitude) == 1) continue;
        if (std::sscanf(line, "sea_level %lf", &seaLevel) == 1) continue;
        if (std::sscanf(line, "seed %llu", reinterpret_cast<unsigned long long*>(&seed)) == 1) continue;
        if (std::sscanf(line, "height %lf %lf %lf %lf %d", &x, &y, &z, &h, &biome) == 5)
        {
            rows.push_back({ x, y, z, h, biome });
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

    double maxErr = 0.0;
    int mismatches = 0;
    int biomeMismatch = 0;

    for (const Row& r : rows)
    {
        const double h = OULT::HeightDir(r.x, r.y, r.z, amplitude, seaLevel, seed);
        const double err = std::fabs(h - r.h);
        if (err > maxErr) maxErr = err;
        if (err > 1e-6)
        {
            ++mismatches;
            if (mismatches <= 3)
            {
                std::printf("  [!] dir (%.3f,%.3f,%.3f): клиент %.10f, сервер %.10f, разница %.3e\n",
                    r.x, r.y, r.z, h, r.h, err);
            }
        }
        const int b = static_cast<int>(OULT::BiomeFromHeight(h, amplitude));
        if (b != r.biome) ++biomeMismatch;
    }

    std::snprintf(buf, sizeof(buf), "высоты совпали бит-в-бит (макс. разница %.2e)", maxErr);
    Check(mismatches == 0, buf);
    std::snprintf(buf, sizeof(buf), "биомы совпали (расхождений %d)", biomeMismatch);
    Check(biomeMismatch == 0, buf);

    std::printf("\n=== Итог ===\n");
    if (g_failures == 0)
    {
        std::printf("Рельеф клиента совпадает с рельефом сервера.\n");
        return 0;
    }
    std::printf("Провалено проверок: %d\n", g_failures);
    return 1;
}
