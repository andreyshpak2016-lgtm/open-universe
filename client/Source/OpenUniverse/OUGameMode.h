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
 */
UCLASS()
class OPENUNIVERSE_API AOUGameMode : public AGameModeBase
{
	GENERATED_BODY()

public:
	AOUGameMode();
};
