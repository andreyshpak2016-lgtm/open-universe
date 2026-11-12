// Copyright Open Universe. Все права защищены.

#include "OUPlayerMenuHUD.h"
#include "OUPlayerMenuController.h"
#include "GameFramework/PlayerController.h"

void AOUPlayerMenuHUD::DrawHUD()
{
	Super::DrawHUD();
	if (!Canvas)
	{
		return;
	}
	APlayerController* PC = PlayerOwner;
	AOUPlayerMenuController* Menu = Cast<AOUPlayerMenuController>(PC);
	if (Menu)
	{
		Menu->DrawMenu(Canvas);
		Menu->DrawBuildMenu(Canvas);
	}
}
