// Copyright Open Universe. Все права защищены.

using UnrealBuildTool;
using System.Collections.Generic;

/// Цель сборки игры (Windows, Linux, Mac).
public class OpenUniverseTarget : TargetRules
{
	public OpenUniverseTarget(TargetInfo Target) : base(Target)
	{
		Type = TargetType.Game;
		DefaultBuildSettings = BuildSettingsVersion.V5;
		IncludeOrderVersion = EngineIncludeOrderVersion.Unreal5_5;
		ExtraModuleNames.Add("OpenUniverse");
	}
}
