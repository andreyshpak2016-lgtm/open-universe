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
		// Сначала палитра строительства, затем игровой HUD: так статусы и прицел
		// всегда сверху, а хотбар не перекрывает палитру (см. DrawGameHud).
		Menu->DrawBuildMenu(Canvas);
		Menu->DrawGameHud(Canvas);
		Menu->DrawMarket(Canvas);
		// Окно скриптов (L) — там же, поверх HUD.
		Menu->DrawScripts(Canvas);
		// Окно мастерской (K).
		Menu->DrawWorkshop(Canvas);
		// Окно организации (O).
		Menu->DrawOrg(Canvas);
		// Пауза-меню (ESC) — поверх всего.
		Menu->DrawPauseMenu(Canvas);
	}
}
