// Copyright Open Universe. Все права защищены.

#pragma once

// ВАЖНО: только стандартный C++ — без UE-типов. Этот файл собирается и внутри
// UE, и обычным компилятором в кросс-проверке (client/testdata/test_terrain.cpp).
// Так одна и та же формула проверяется автотестом независимо от движка.

#include <cmath>
#include <cstdint>

/**
 * Общий рельеф планеты: ОДНА формула для сервера и клиента.
 *
 * ## Зачем отдельный файл
 * Высота поверхности — это данные мира: по ним сервер проверяет посадку и
 * столкновения, а клиент рисует ту же землю. Если формулы разойдутся, игрок
 * «утонет» в холме или повиснет над землёй. Поэтому формула описана **один раз**
 * здесь (на клиенте) и зеркально в `server/crates/sim/src/terrain.rs`.
 *
 * ## Точность
 * Используются только `+ - * /`, `floor` и сравнения — эти операции в IEEE-754
 * дают **бит-в-бит одинаковый** результат в Rust (f64) и C++ (double).
 * Тригонометрия и `sqrt` не используются намеренно (платформо-зависимы).
 *
 * ## Почему шум трёхмерный
 * Шум считается **по направлению в 3D**, а не по развёртке сферы на плоскость.
 * Отображение сферы на квадрат рвёт производную на «рёбрах», и на планете
 * появляются складки/швы. 3D-шум непрерывен на всей сфере.
 *
 * ## Единицы
 * Вход — направление от центра планеты (не обязательно единичное), выход —
 * высота над уровнем океана в тех же единицах, что `amplitude`.
 */
namespace OULT
{
	/** Версия формулы рельефа. Должна совпадать с `ou_sim::TERRAIN_VERSION`. */
	constexpr int32_t TerrainVersion = 4;

	/** Ограничение значения в диапазон (аналог clamp без зависимостей). */
	inline double Clamp(double V, double Lo, double Hi)
	{
		if (V < Lo) return Lo;
		if (V > Hi) return Hi;
		return V;
	}

	/**
	 * Хеш решётки в [0,1). Зеркало серверного `lattice3`.
	 *
	 * ВАЖНО: вычисления — в `uint64` с переполнением (wrapping), как в Rust:
	 * переполнение допустимо, оно и даёт тот же результат.
	 */
	inline double Lattice3(int64_t X, int64_t Y, int64_t Z, uint64_t Seed)
	{
		uint64_t H = Seed;
		const int64_t Coords[3] = { X, Y, Z };
		for (int64_t C : Coords)
		{
			H ^= static_cast<uint64_t>(C) * 0x9E3779B97F4A7C15ull;
			H = (H ^ (H >> 30)) * 0xBF58476D1CE4E5B9ull;
			H = (H ^ (H >> 27)) * 0x94D049BB133111EBull;
		}
		return static_cast<double>(H >> 11) / static_cast<double>(1ull << 53);
	}

	inline double Smooth(double T)
	{
		return T * T * (3.0 - 2.0 * T);
	}

	/** Трёхмерный value-noise с трилинейной интерполяцией. Зеркало `noise3`. */
	inline double Noise3(double X, double Y, double Z, uint64_t Seed)
	{
		const double XIf = std::floor(X);
		const double YIf = std::floor(Y);
		const double ZIf = std::floor(Z);
		const double FX = Smooth(X - XIf);
		const double FY = Smooth(Y - YIf);
		const double FZ = Smooth(Z - ZIf);
		const int64_t XI = static_cast<int64_t>(XIf);
		const int64_t YI = static_cast<int64_t>(YIf);
		const int64_t ZI = static_cast<int64_t>(ZIf);

		const double C000 = Lattice3(XI,     YI,     ZI,     Seed);
		const double C100 = Lattice3(XI + 1, YI,     ZI,     Seed);
		const double C010 = Lattice3(XI,     YI + 1, ZI,     Seed);
		const double C110 = Lattice3(XI + 1, YI + 1, ZI,     Seed);
		const double C001 = Lattice3(XI,     YI,     ZI + 1, Seed);
		const double C101 = Lattice3(XI + 1, YI,     ZI + 1, Seed);
		const double C011 = Lattice3(XI,     YI + 1, ZI + 1, Seed);
		const double C111 = Lattice3(XI + 1, YI + 1, ZI + 1, Seed);

		const double X00 = C000 + (C100 - C000) * FX;
		const double X10 = C010 + (C110 - C010) * FX;
		const double X01 = C001 + (C101 - C001) * FX;
		const double X11 = C011 + (C111 - C011) * FX;
		const double Y0 = X00 + (X10 - X00) * FY;
		const double Y1 = X01 + (X11 - X01) * FY;
		return Y0 + (Y1 - Y0) * FZ;
	}

	/** Фрактальный шум (fBm): сумма октав 3D-шума. Зеркало серверного `fbm`. */
	inline double Fbm(double X, double Y, double Z, int32_t Octaves, double BaseFreq, uint64_t Seed)
	{
		double V = 0.0, Amp = 1.0, Freq = BaseFreq, Norm = 0.0;
		for (int32_t I = 0; I < Octaves; ++I)
		{
			V += Noise3(X * Freq, Y * Freq, Z * Freq,
				Seed ^ (0x1234ull * static_cast<uint64_t>(I + 1))) * Amp;
			Norm += Amp;
			Amp *= 0.5;
			Freq *= 2.0;
		}
		return V / Norm;
	}

	/** Хребтовый шум (горные хребты). Зеркало серверного `ridged`. */
	inline double Ridged(double X, double Y, double Z, int32_t Octaves, double BaseFreq, uint64_t Seed)
	{
		double V = 0.0, Amp = 1.0, Freq = BaseFreq, Norm = 0.0;
		for (int32_t I = 0; I < Octaves; ++I)
		{
			const double N = Noise3(X * Freq, Y * Freq, Z * Freq,
				Seed ^ (0x5000ull + 0x77ull * static_cast<uint64_t>(I + 1)));
			const double R = 1.0 - std::fabs(2.0 * N - 1.0);
			V += R * R * Amp;
			Norm += Amp;
			Amp *= 0.5;
			Freq *= 2.1;
		}
		return V / Norm;
	}

	/**
	 * Высота над уровнем океана в направлении. Зеркало серверного
	 * `Terrain::height`. Направление можно передавать неединичным.
	 */
	inline double HeightDir(
		double X, double Y, double Z,
		double Amplitude, double SeaLevel, uint64_t Seed)
	{
		// Нормализуем направление: делим на длину, ровно как серверный
		// `normalize`. sqrt по IEEE-754 детерминирован и одинаков в Rust и C++.
		const double Len = std::sqrt(X * X + Y * Y + Z * Z);
		if (Len > 1e-12)
		{
			X /= Len; Y /= Len; Z /= Len;
		}

		// Форма континентов (крупный шум).
		const double Cont = Fbm(X, Y, Z, 4, 0.9, Seed ^ 0xC2B2ull);
		// Холмы: длина волны заметно больше шага меша планеты (~3 км),
		// иначе меш не может их отобразить и «алиасит» большими плитами.
		const double Mid = Fbm(X, Y, Z, 4, 20.0, Seed ^ 0xA11CEull);
		// Мелкий рельеф: холмы в единицы километров.
		const double Fine = Fbm(X, Y, Z, 4, 150.0, Seed ^ 0xD37ull);
		// Горы: острые хребты.
		const double Mtn = Ridged(X, Y, Z, 3, 4.5, Seed ^ 0x5EEDull);

		// Отклонение от уровня океана.
		const double Base = Cont - SeaLevel;
		// Горы «включаются» только в глубине материка (не у берега).
		const double Land = Clamp(Base / 0.09, 0.0, 1.0);

		// Двусторонний рельеф: холмы вверх и вниз, плюс горы на материках.
		// Амплитуда подобрана так, чтобы линейная интерполяция меша (шаг ~1.5 км)
		// не «вспучивалась» выше роста камеры (1.66 м): иначе игрок оказывается
		// под гранью и видит изнанку поверхности.
		const double H01 = Base
			+ (Mid - 0.5) * 0.16 * (0.3 + 0.7 * Land)
			+ (Fine - 0.5) * 0.020 * (0.2 + 0.8 * Land)
			+ Mtn * Land * 0.34;
		return H01 * Amplitude * 2.0;
	}

	/** Коды биомов (совпадают с серверными `Biome::code`). */
	enum EBiome : uint8_t
	{
		BiomeOcean = 0,
		BiomeBeach = 1,
		BiomePlains = 2,
		BiomeHills = 3,
		BiomeMountains = 4,
	};

	/** Биом по высоте над уровнем океана. Зеркало `Terrain::biome`. */
	inline int32_t BiomeFromHeight(double Height, double Amplitude)
	{
		if (Height <= 0.0) return BiomeOcean;
		if (Height < Amplitude * 0.04) return BiomeBeach;
		if (Height < Amplitude * 0.30) return BiomePlains;
		if (Height < Amplitude * 0.55) return BiomeHills;
		return BiomeMountains;
	}
}
