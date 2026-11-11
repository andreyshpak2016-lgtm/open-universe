// Copyright Open Universe. Все права защищены.

using UnrealBuildTool;
using System.Collections.Generic;

/// Цель сборки редактора.
public class OpenUniverseEditorTarget : TargetRules
{
	public OpenUniverseEditorTarget(TargetInfo Target) : base(Target)
	{
		Type = TargetType.Editor;
		DefaultBuildSettings = BuildSettingsVersion.V5;
		IncludeOrderVersion = EngineIncludeOrderVersion.Unreal5_5;
		ExtraModuleNames.Add("OpenUniverse");
	}
}
