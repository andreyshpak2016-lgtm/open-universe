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
	constexpr int32_t TerrainVersion = 5;

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
	 *
	 * ## Уровни детализации (`Detail`)
	 * Планета огромна (радиус 500 км), а её глобальный меш — ~1.5 км на ячейку.
	 * Такой меш физически не может нарисовать рельеф мельче ~3–5 км: слишком
	 * мелкие волны он «алиасит» огромными плитами, и камера оказывается под
	 * гранью (серая изнанка поверхности). Поэтому рельеф разделён на два уровня:
	 *
	 * - `Detail = 0` (**крупный**, глобальный меш): только волны длиннее шага
	 *   меша — континенты, крупные холмы и горные хребты. Именно это видно
	 *   издалека и на всей планете.
	 * - `Detail = 1` (**полный**, физика сервера и локальный участок): к крупному
	 *   добавляется мелкий рельеф (сотни метров). Его показывает только плотная
	 *   сетка у игрока — там шаг ~60 м, и мелочь видна без искажений.
	 *
	 * Мелкий рельеф **неотрицателен** (квадраты шума): он только приподнимает
	 * землю над крупной формой. Так локальный участок всегда лежит НАД глобальным
	 * мешем и не «прошивается» им насквозь (нет мерцания и дыр).
	 */
	inline double HeightDir(
		double X, double Y, double Z,
		double Amplitude, double SeaLevel, uint64_t Seed, int32_t Detail = 1)
	{
		// Нормализуем направление: делим на длину, ровно как серверный
		// `normalize`. sqrt по IEEE-754 детерминирован и одинаков в Rust и C++.
		const double Len = std::sqrt(X * X + Y * Y + Z * Z);
		if (Len > 1e-12)
		{
			X /= Len; Y /= Len; Z /= Len;
		}

		// --- Крупный рельеф: виден на глобальном меше (шаг ~1.5 км) ---
		// Форма континентов (крупный шум): где `Cont > SeaLevel` — суша.
		const double Cont = Fbm(X, Y, Z, 4, 0.9, Seed ^ 0xC2B2ull);
		// Крупные холмы: длина волны десятки километров — меш их разрешает
		// (волны мельче шага меша он «алиасил» бы огромными плитами).
		const double Hills = Fbm(X, Y, Z, 3, 4.0, Seed ^ 0xA11CEull);
		// Горы: острые хребты (длина волны ~50–100 км).
		const double Mtn = Ridged(X, Y, Z, 2, 2.5, Seed ^ 0x5EEDull);

		// Отклонение от уровня океана.
		const double Base = Cont - SeaLevel;
		// Горы и холмы «включаются» только в глубине материка (не у берега).
		const double Land = Clamp(Base / 0.09, 0.0, 1.0);
		const double Lush = 0.3 + 0.7 * Land;

		// Двусторонний рельеф: холмы вверх и вниз, плюс горы на материках.
		// Амплитуда подобрана так, чтобы линейная интерполяция меша (шаг ~1.5 км)
		// «вспучивалась» не выше роста камеры (1.66 м): иначе игрок оказывается
		// под гранью и видит изнанку поверхности (серая стена).
		double H01 = Base
			+ (Hills - 0.5) * 0.24 * Lush
			+ Mtn * Land * 0.34;

		// --- Мелкий рельеф: только для плотной сетки у игрока ---
		// Эти волны (сотни метров — единицы километров) глобальный меш не
		// разрешает, поэтому их показывает лишь локальный участок (шаг ~60 м),
		// а сервер использует их для посадки. Квадраты шума дают «бугры» в [0,1]:
		// мелочь только приподнимает землю, поэтому локальный участок всегда
		// лежит НАД глобальным мешем и не «прошивается» им насквозь.
		if (Detail >= 1)
		{
			const double B1 = Fbm(X, Y, Z, 2, 400.0, Seed ^ 0xD37ull);
			const double B2 = Fbm(X, Y, Z, 2, 800.0, Seed ^ 0xF1Eull);
			const double B3 = Fbm(X, Y, Z, 2, 2000.0, Seed ^ 0xA5A5ull);
			H01 += (0.0025
				+ B1 * B1 * 0.050
				+ B2 * B2 * 0.020
				+ B3 * B3 * 0.008) * Lush;
		}
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

	/**
	 * Воронка (ямa), выкопанная игроком.
	 *
	 * Рельеф — чистая функция направления; правки игроков хранятся отдельным
	 * списком воронок и **применяются поверх** базовой формулы. Так сервер и
	 * клиент считают один и тот же изменённый рельеф: сервер — для физики,
	 * клиент — для картинки. Зеркало серверной `Dent`.
	 */
	struct Dent
	{
		/** Направление центра воронки (серверные оси). */
		double X = 0.0, Y = 0.0, Z = 0.0;
		/** Косинус углового радиуса: точки с dot(dir) ≥ CosR «внутри» воронки. */
		double CosR = 1.0;
		/** Глубина в единицах длины (сколько снять в центре). */
		double Depth = 0.0;
	};

	/**
	 * Насколько список воронок понижает рельеф в направлении `Dx,Dy,Dz`.
	 *
	 * Возвращает НЕОТРИЦАТЕЛЬНУЮ величину (в тех же единицах, что `Amplitude`).
	 * Форма — квадратичная: у края воронки глубина 0, в центре — `Depth`.
	 * Зеркало серверной `Dents::offset`.
	 */
	inline double DentsOffset(
		double Dx, double Dy, double Dz,
		const Dent* Dents, int32_t Count)
	{
		const double Len = std::sqrt(Dx * Dx + Dy * Dy + Dz * Dz);
		if (Len > 1e-12) { Dx /= Len; Dy /= Len; Dz /= Len; }
		double Sum = 0.0;
		for (int32_t I = 0; I < Count; ++I)
		{
			const Dent& D = Dents[I];
			const double K = Dx * D.X + Dy * D.Y + Dz * D.Z;   // cos угла
			if (K >= D.CosR)
			{
				const double Span = 1.0 - D.CosR;
				const double T = (Span > 1e-12) ? ((K - D.CosR) / Span) : 1.0;
				Sum += D.Depth * T * T;
			}
		}
		return Sum;
	}

	/**
	 * Высота рельефа с учётом воронок игроков.
	 *
	 * Базовая высота из [`HeightDir`] минус сумма воронок. Воронки применяются
	 * к ПОЛНОМУ рельефу (detail=1): копание влияет на физику и плотный участок.
	 */
	inline double HeightWithDents(
		double X, double Y, double Z,
		double Amplitude, double SeaLevel, uint64_t Seed, int32_t Detail,
		const Dent* Dents, int32_t Count)
	{
		const double Base = HeightDir(X, Y, Z, Amplitude, SeaLevel, Seed, Detail);
		if (Count <= 0)
		{
			return Base;
		}
		return Base - DentsOffset(X, Y, Z, Dents, Count);
	}

	/** Биом по высоте над уровнем океана. Зеркало `Terrain::biome`. */
	inline int32_t BiomeFromHeight(double Height, double Amplitude)
	{
		if (Height <= 0.0) return BiomeOcean;
		if (Height < Amplitude * 0.04) return BiomeBeach;
		if (Height < Amplitude * 0.30) return BiomePlains;
		if (Height < Amplitude * 0.55) return BiomeHills;
		return BiomeMountains;
	}

	// -----------------------------------------------------------------------
	// Детерминированная тригонометрия (зеркало серверных `det_sin`/`det_cos`).
	// Нужна, чтобы выбор точки появления совпал с сервером бит-в-бит.
	// -----------------------------------------------------------------------

	/** Приведение угла к [−π, π] только арифметикой (как `wrap_pi`). */
	inline double WrapPi(double X)
	{
		const double TwoPi = 6.283185307179586;
		X -= TwoPi * std::round(X / TwoPi);
		if (X > 3.141592653589793) X -= TwoPi;
		else if (X < -3.141592653589793) X += TwoPi;
		return X;
	}

	/** Детерминированный синус — полином Тейлора (как серверный `det_sin`). */
	inline double DetSin(double X)
	{
		const double A = WrapPi(X);
		const double A2 = A * A;
		double Term = A, Sum = A;
		Term = -Term * A2 / 6.0;   Sum += Term;
		Term = -Term * A2 / 20.0;  Sum += Term;
		Term = -Term * A2 / 42.0;  Sum += Term;
		Term = -Term * A2 / 72.0;  Sum += Term;
		Term = -Term * A2 / 110.0; Sum += Term;
		Term = -Term * A2 / 156.0; Sum += Term;
		Term = -Term * A2 / 210.0; Sum += Term;
		Term = -Term * A2 / 272.0; Sum += Term;
		return Sum;
	}

	/** Детерминированный косинус (как серверный `det_cos`). */
	inline double DetCos(double X)
	{
		return DetSin(X + 3.141592653589793 / 2.0);
	}

	/** Радиус ровной площадки, которую ищем под базу и точку появления, метры. */
	constexpr double SpawnFlatRadiusM = 150.0;

	/**
	 * Перепад высот вокруг направления `D` в радиусе `RadiusM` метров.
	 * Зеркало серверной `spawn_roughness`. Направление `D` — серверные оси.
	 */
	inline double SpawnRoughness(
		double Dx, double Dy, double Dz,
		double RadiusM, double Amplitude, double SeaLevel, uint64_t Seed)
	{
		const double Len = std::sqrt(Dx * Dx + Dy * Dy + Dz * Dz);
		if (Len > 1e-12) { Dx /= Len; Dy /= Len; Dz /= Len; }
		// Радиус планеты в «единицах высоты»: амплитуда рельефа задана так, что
		// высота = h01 · Amplitude · 2. Радиус берём из константы планеты.
		const double PlanetRadius = 500000.0;
		const double E = RadiusM / PlanetRadius;
		double Hi = HeightDir(Dx, Dy, Dz, Amplitude, SeaLevel, Seed, 1);
		double Lo = Hi;
		const int32_t N = 8;
		for (int32_t K = 0; K < N; ++K)
		{
			const double Ang = 6.283185307179586 * static_cast<double>(K) / static_cast<double>(N);
			const double C = DetCos(Ang);
			const double S = DetSin(Ang);
			const double Offs[3][3] = {
				{ C, S, 0.0 }, { C, 0.0, S }, { 0.0, C, S }
			};
			for (int32_t O = 0; O < 3; ++O)
			{
				const double H = HeightDir(
					Dx + Offs[O][0] * E, Dy + Offs[O][1] * E, Dz + Offs[O][2] * E,
					Amplitude, SeaLevel, Seed, 1);
				if (H > Hi) Hi = H;
				if (H < Lo) Lo = H;
			}
		}
		return Hi - Lo;
	}

	/**
	 * Направление точки появления: **самое ровное** место на суше.
	 * Зеркало серверной `home_spawn_direction` (серверные оси).
	 *
	 * Возвращает единичный вектор (серверные оси). Детерминировано: тот же обход
	 * и та же функция ровности, что на сервере.
	 */
	inline void HomeSpawnDirectionServer(
		double Amplitude, double SeaLevel, uint64_t Seed,
		double& OutX, double& OutY, double& OutZ)
	{
		const double Step = 0.05;
		double BestRough = 1e300;
		double Bx = 0.0, By = 1.0, Bz = 0.0;
		bool bFound = false;
		for (double Phi = -1.2; Phi <= 1.2; Phi += Step)
		{
			for (double Theta = 0.0; Theta < 6.283185307179586; Theta += Step)
			{
				const double Dx = DetCos(Phi) * DetCos(Theta);
				const double Dy = DetSin(Phi);
				const double Dz = DetCos(Phi) * DetSin(Theta);
				const double H = HeightDir(Dx, Dy, Dz, Amplitude, SeaLevel, Seed, 1);
				if (H > Amplitude * 0.15 && H < Amplitude * 0.9)
				{
					const double R = SpawnRoughness(Dx, Dy, Dz, SpawnFlatRadiusM, Amplitude, SeaLevel, Seed);
					if (!bFound || R < BestRough)
					{
						BestRough = R;
						const double L = std::sqrt(Dx * Dx + Dy * Dy + Dz * Dz);
						Bx = Dx / L; By = Dy / L; Bz = Dz / L;
						bFound = true;
					}
				}
			}
		}
		OutX = Bx; OutY = By; OutZ = Bz;
	}
}
