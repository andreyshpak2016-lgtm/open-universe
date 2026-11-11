// Copyright Open Universe. Все права защищены.

#pragma once

#include "CoreMinimal.h"
#include "GameFramework/GameModeBase.h"
#include "OUGameMode.generated.h"

/**
 * Режим игры Open Universe.
 *
 * В MMO авторитет — на сервере (Rust), поэтому UE-сервер здесь не главный:
 * этот GameMode управляет только клиентской сценой (миром, камерой, HUD).
 *
 * При старте режим **достраивает недостающую сцену**: свет, небо, туман,
 * планету и менеджер начала координат. Это страховка: даже если карта пустая
 * (как сразу после первого открытия проекта), мир всё равно появится —
 * больше не будет чёрного экрана.
 */
UCLASS()
class OPENUNIVERSE_API AOUGameMode : public AGameModeBase
{
	GENERATED_BODY()

public:
	AOUGameMode();

	/** Достроить сцену, если в мире не хватает ключевых акторов. */
	virtual void StartPlay() override;

	/** Точка старта: если на карте её нет — создаём сами (иначе будет чёрный экран). */
	virtual AActor* ChoosePlayerStart_Implementation(AController* Player) override;

	/** Собрать всю сцену заново: свет, небо, туман, планета, начало координат. */
	UFUNCTION(BlueprintCallable, CallInEditor, Category = "Open Universe|World")
	void BuildWorld();

protected:
	virtual void BeginPlay() override;
};
