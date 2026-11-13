# -*- coding: utf-8 -*-
"""Генератор материалов базы Open Universe для UE5.

Создаёт /Game/Materials/M_BasePanel — параметрический материал для построек
базы: поверхность с панельными линиями (процедурная сетка по мировым
координатам) и параметром цвета `Color`, который код выставляет каждому блоку.

Материал собран из узлов MaterialExpression (без внешних текстур), поэтому
работает сразу после сборки — как и остальной проект.

Запуск (см. tools/make-materials.ps1):
  UnrealEditor-Cmd.exe <project> -run=pythonscript -script="tools/make-materials.py"
"""

import unreal

MAT_PATH = "/Game/Materials"
MAT_NAME = "M_BasePanel"
MAT_FULL = MAT_PATH + "/" + MAT_NAME


def log(msg):
    unreal.log("[OpenUniverse] " + msg)


def make_material():
    mel = unreal.MaterialEditingLibrary
    if not mel:
        log("нет MaterialEditingLibrary — пропускаю материалы")
        return

    if unreal.EditorAssetLibrary.does_asset_exist(MAT_FULL):
        unreal.EditorAssetLibrary.delete_asset(MAT_FULL)

    asset_tools = unreal.AssetToolsHelpers.get_asset_tools()
    mat = asset_tools.create_asset(MAT_NAME, MAT_PATH, unreal.Material, None)
    if mat is None:
        log("не создать материал")
        return

    def expr(cls, x, y):
        return mel.create_material_expression(mat, cls, x, y)

    # Цвет блока (код выставляет его параметром).
    color = expr(unreal.MaterialExpressionVectorParameter, -900, 0)
    color.set_editor_property("parameter_name", "Color")
    color.set_editor_property("default_value", unreal.LinearColor(0.4, 0.45, 0.5, 1.0))

    # Панельные линии: WorldPosition.xy * 0.0033 → frac → «ромб» по панели.
    wp = expr(unreal.MaterialExpressionWorldPosition, -1300, 300)
    mask = expr(unreal.MaterialExpressionComponentMask, -1120, 300)
    mask.set_editor_property("r", True)
    mask.set_editor_property("g", True)
    mask.set_editor_property("b", False)
    mask.set_editor_property("a", False)
    mel.connect_material_expressions(wp, "", mask, "")

    mul = expr(unreal.MaterialExpressionMultiply, -940, 300)
    mul.set_editor_property("const_b", 0.0033)
    mel.connect_material_expressions(mask, "", mul, "A")

    frac = expr(unreal.MaterialExpressionFrac, -780, 300)
    mel.connect_material_expressions(mul, "", frac, "")

    twice = expr(unreal.MaterialExpressionMultiply, -620, 300)
    twice.set_editor_property("const_b", 2.0)
    mel.connect_material_expressions(frac, "", twice, "A")

    one_minus = expr(unreal.MaterialExpressionSubtract, -460, 300)
    one_minus.set_editor_property("const_b", 1.0)
    mel.connect_material_expressions(twice, "", one_minus, "A")

    absn = expr(unreal.MaterialExpressionAbs, -300, 300)
    mel.connect_material_expressions(one_minus, "", absn, "")

    # seam ∈ [0,1]: 1 в центре панели, 0 на швах.
    seam = expr(unreal.MaterialExpressionSubtract, -140, 300)
    seam.set_editor_property("const_b", 1.0)
    mel.connect_material_expressions(absn, "", seam, "A")

    # Затемнение: brightness = 0.65 + 0.35 * seam.
    bright_mul = expr(unreal.MaterialExpressionMultiply, 20, 300)
    bright_mul.set_editor_property("const_b", 0.35)
    mel.connect_material_expressions(seam, "", bright_mul, "A")

    bright = expr(unreal.MaterialExpressionAdd, 180, 300)
    bright.set_editor_property("const_b", 0.65)
    mel.connect_material_expressions(bright_mul, "", bright, "A")

    # BaseColor = Color * brightness (Color — вектор, brightness — скаляр).
    out_col = expr(unreal.MaterialExpressionMultiply, 360, 120)
    mel.connect_material_expressions(color, "", out_col, "A")
    mel.connect_material_expressions(bright, "", out_col, "B")
    mel.connect_material_property(out_col, "", unreal.MaterialProperty.MP_BASE_COLOR)

    # Шероховатость и металличность — параметрами.
    rough = expr(unreal.MaterialExpressionScalarParameter, 360, 420)
    rough.set_editor_property("parameter_name", "Roughness")
    rough.set_editor_property("default_value", 0.7)
    mel.connect_material_property(rough, "", unreal.MaterialProperty.MP_ROUGHNESS)

    metal = expr(unreal.MaterialExpressionScalarParameter, 360, 560)
    metal.set_editor_property("parameter_name", "Metallic")
    metal.set_editor_property("default_value", 0.1)
    mel.connect_material_property(metal, "", unreal.MaterialProperty.MP_METALLIC)

    mel.recompile_material(mat)
    unreal.EditorAssetLibrary.save_asset(MAT_FULL)
    log("материал создан: " + MAT_FULL)


def main():
    make_material()


main()
