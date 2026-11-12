// Copyright Open Universe. Все права защищены.

#pragma once

#include "CoreMinimal.h"
#include "GameFramework/HUD.h"
#include "OUPlayerMenuHUD.generated.h"

/**
 * HUD, который рисует стартовое меню.
 *
 * Логика меню (состояние, ввод, подключение) — в `AOUPlayerMenuController`,
 * а рисование канвасом — здесь: у `APlayerController` нет `DrawHUD`, его
 * вызывает `AHUD`. Разделение держит контроллер свободным от графики.
 */
UCLASS()
class OPENUNIVERSE_API AOUPlayerMenuHUD : public AHUD
{
	GENERATED_BODY()

public:
	virtual void DrawHUD() override;
};
