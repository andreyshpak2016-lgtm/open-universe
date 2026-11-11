// Copyright Open Universe. Все права защищены.

#pragma once

#include "CoreMinimal.h"
#include "Math/UnrealMathUtility.h"

/**
 * Детерминированная тригонометрия.
 *
 * ## Зачем не `FMath::Sin`
 * `FMath::Sin` может считаться по-разному на разных платформах и версиях
 * движка. Для предсказания и реконсиляции клиент обязан считать мир бит-в-бит
 * как сервер. Наш сервер использует свой полином (`crates/sim::det_sin`),
 * поэтому клиент повторяет **ту же формулу**.
 *
 * Формула: приведение угла к [−π, π], затем ряд Тейлора до x^17.
 */
namespace OUDeterministic
{
	/** Число π (совпадает с сервером). */
	constexpr double OUPi = 3.141592653589793;
	constexpr double OUTwoPi = 6.283185307179586;

	/** Приведение угла к диапазону [−π, π] только арифметикой. */
	FORCEINLINE double WrapPi(double X)
	{
		X -= OUTwoPi * FMath::RoundToDouble(X / OUTwoPi);
		if (X > OUPi)
		{
			X -= OUTwoPi;
		}
		else if (X < -OUPi)
		{
			X += OUTwoPi;
		}
		return X;
	}

	/**
	 * Детерминированный синус — полином Тейлора (как на сервере).
	 * Только `+ - * /`, которые IEEE-754 задаёт однозначно.
	 */
	FORCEINLINE double DetSin(double X)
	{
		const double A = WrapPi(X);
		const double A2 = A * A;
		double Term = A;
		double Sum = A;
		Term = -Term * A2 / 6.0;
		Sum += Term;
		Term = -Term * A2 / 20.0;
		Sum += Term;
		Term = -Term * A2 / 42.0;
		Sum += Term;
		Term = -Term * A2 / 72.0;
		Sum += Term;
		Term = -Term * A2 / 110.0;
		Sum += Term;
		Term = -Term * A2 / 156.0;
		Sum += Term;
		Term = -Term * A2 / 210.0;
		Sum += Term;
		Term = -Term * A2 / 272.0;
		Sum += Term;
		return Sum;
	}

	/** Детерминированный косинус через сдвиг синуса. */
	FORCEINLINE double DetCos(double X)
	{
		return DetSin(X + OUPi / 2.0);
	}
}
