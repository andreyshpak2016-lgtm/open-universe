// Copyright Open Universe. Все права защищены.

#pragma once

#include "CoreMinimal.h"
#include "OUHealthTypes.generated.h"

/**
 * Статус «сервер + клиент» Open Universe.
 *
 * Читается с сайта (`status.json`) — тот обновляется регулярной проверкой
 * (GitHub Actions) или вручную через `tools/health.ps1`. Показывается в меню
 * клиента и в лаунчере, чтобы игрок видел, работает ли инфраструктура.
 */
USTRUCT(BlueprintType)
struct FOUHealthStatus
{
	GENERATED_BODY()

	/** Известен ли статус (удалось ли прочитать файл). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Health")
	bool bKnown = false;

	/** Общий вердикт: всё ли работает. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Health")
	bool bOk = false;

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Health")
	bool bServer = false;

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Health")
	bool bClient = false;

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Health")
	bool bSimulation = false;

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Health")
	bool bSpawnOnPlanet = false;

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Health")
	int32 Tick = 0;

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Health")
	float LatencyMs = 0.f;

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Health")
	FString Notes;
};
