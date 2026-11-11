// Copyright Open Universe. Все права защищены.

#include "OUGameMode.h"
#include "OUShipPawn.h"
#include "OUOriginManager.h"

AOUGameMode::AOUGameMode()
{
	// Игрок управляет кораблём; мир — общий, начало координат подвижное.
	DefaultPawnClass = AOUShipPawn::StaticClass();
	// Менеджер начала координат создаётся как актор уровня, не как спавн.
}
