// Copyright Open Universe. Все права защищены.

#include "OpenUniverse.h"
#include "Modules/ModuleManager.h"

DEFINE_LOG_CATEGORY_STATIC(LogOpenUniverse, Log, All);

void FOpenUniverseModule::StartupModule()
{
	UE_LOG(LogOpenUniverse, Log, TEXT("Open Universe: модуль запущен"));
}

void FOpenUniverseModule::ShutdownModule()
{
	UE_LOG(LogOpenUniverse, Log, TEXT("Open Universe: модуль остановлен"));
}

// Точка регистрации модуля для UE.
IMPLEMENT_PRIMARY_GAME_MODULE(FOpenUniverseModule, OpenUniverse, "OpenUniverse");
