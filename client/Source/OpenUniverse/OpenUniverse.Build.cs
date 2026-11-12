// Copyright Open Universe. Все права защищены.

using UnrealBuildTool;

public class OpenUniverse : ModuleRules
{
	public OpenUniverse(ReadOnlyTargetRules Target) : base(Target)
	{
		PCHUsage = PCHUsageMode.UseExplicitOrSharedPCHs;

		// Public — заголовки, видимые другим модулям; Private — реализация.
		PublicDependencyModuleNames.AddRange(new string[]
		{
			"Core",
			"CoreUObject",
			"Engine",
			"InputCore",
			"NetCore",        // сетевые типы
			"Sockets",        // UDP-сокеты
			"Networking",     // транспорт
			"ProceduralMeshComponent", // воксельные планеты (меши)
			"HTTP",           // чтение status.json (статус сервер+клиент)
			"Json",           // разбор статуса
			"JsonUtilities"
		});

		PrivateDependencyModuleNames.AddRange(new string[]
		{
			"RenderCore",
			"RHI"
		});

		// Мост «Python → UE Landscape» (OULandscapeBuilder) — только редактору:
		// нужен рантайм-модуль Landscape и редакторные типы (GEditor, импорт).
		// В подписях UHT используются только AActor/UMaterialInterface, поэтому
		// игровая сборка модуль Landscape не тянет.
		if (Target.bBuildEditor)
		{
			PrivateDependencyModuleNames.AddRange(new string[]
			{
				"Landscape",
				"UnrealEd",
				"EditorFramework",
				"Foliage"      // LandscapeEdit.h включает InstancedFoliageActor.h
			});
		}

		// Лямбды и C++20 — как в остальном проекте.
		bEnableExceptions = false;
		CppStandard = CppStandardVersion.Cpp20;
	}
}
