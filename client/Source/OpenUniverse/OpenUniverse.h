// Copyright Open Universe. Все права защищены.

#pragma once

#include "CoreMinimal.h"
#include "Modules/ModuleManager.h"

/**
 * Главный модуль игры Open Universe.
 *
 * Регистрирует типы и подготавливает подсистемы. Точка входа для C++-логики,
 * которая связывает сервер (Rust) с рендером (UE5).
 */
class FOpenUniverseModule : public IModuleInterface
{
public:
	/** Вызывается при загрузке модуля. */
	virtual void StartupModule() override;

	/** Вызывается при выгрузке модуля. */
	virtual void ShutdownModule() override;
};
