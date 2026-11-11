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
			"ProceduralMeshComponent" // воксельные планеты (меши)
		});

		PrivateDependencyModuleNames.AddRange(new string[]
		{
			"RenderCore",
			"RHI"
		});

		// Лямбды и C++20 — как в остальном проекте.
		bEnableExceptions = false;
		CppStandard = CppStandardVersion.Cpp20;
	}
}
