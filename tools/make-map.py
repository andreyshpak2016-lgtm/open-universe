# -*- coding: utf-8 -*-
"""
Генератор карты Open Universe для Unreal Engine 5.

Создаёт уровень /Game/Maps/OpenUniverse со светом, небом, туманом,
плановой точкой старта, воксельной планетой и менеджером начала координат,
после чего сохраняет его. Без этого редактор открывает пустую карту —
и получается чёрный экран.

Запуск (см. tools/make-map.ps1):
  UnrealEditor-Cmd.exe <project> -run=pythonscript -script="tools/make-map.py"
"""

import unreal

MAP_PATH = "/Game/Maps/OpenUniverse"


def log(msg):
    unreal.log("[OpenUniverse] " + msg)


def spawn(actor_class_path, location=None, rotation=None):
    cls = unreal.load_class(None, actor_class_path)
    if cls is None:
        log("не найден класс: " + actor_class_path)
        return None
    loc = location if location is not None else unreal.Vector(0.0, 0.0, 0.0)
    rot = rotation if rotation is not None else unreal.Rotator(0.0, 0.0, 0.0)
    actor = unreal.EditorLevelLibrary.spawn_actor_from_class(cls, loc, rot)
    log("создан " + actor_class_path)
    return actor


def main():
    les = unreal.get_editor_subsystem(unreal.LevelEditorSubsystem)
    # Если карта уже существует — удаляем, иначе new_level не сможет её создать.
    if unreal.EditorAssetLibrary.does_asset_exist(MAP_PATH):
        log("удаляю существующую карту: " + MAP_PATH)
        unreal.EditorAssetLibrary.delete_asset(MAP_PATH)
    # Новый пустой уровень.
    les.new_level(MAP_PATH)
    log("новый уровень: " + MAP_PATH)

    # --- Небо и атмосфера ---
    spawn("/Script/Engine.SkyAtmosphere")

    # --- Солнце ---
    sun = spawn("/Script/Engine.DirectionalLight",
                unreal.Vector(0.0, 0.0, 200000.0),
                unreal.Rotator(-45.0, -35.0, 0.0))
    if sun:
        comp = sun.get_editor_property("light_component")
        if comp:
            comp.set_editor_property("mobility", unreal.ComponentMobility.MOVABLE)
            comp.set_editor_property("intensity", 4.0)

    # --- Рассеянный свет неба ---
    sky = spawn("/Script/Engine.SkyLight")
    if sky:
        comp = sky.get_editor_property("light_component")
        if comp:
            comp.set_editor_property("mobility", unreal.ComponentMobility.MOVABLE)
            comp.set_editor_property("real_time_capture", True)

    # --- Атмосферный туман НЕ создаём: в космосе он скрывает планету ---

    # --- Точка старта игрока ---
    # Точную позицию на поверхности планеты задаёт GameMode (ChoosePlayerStart):
    # он ставит игрока на сушу. Здесь — лишь запасная точка.
    spawn("/Script/Engine.PlayerStart", unreal.Vector(0.0, 0.0, 0.0),
          unreal.Rotator(-20.0, 0.0, 0.0))

    # --- Планету НЕ создаём: в мире она одна, и её строит GameMode ---
    # (раньше здесь была декоративная планета; теперь система сведена к одной
    # большой планете, которую создаёт клиент из общих с сервером констант).

    # --- Менеджер начала координат (floating origin) ---
    # Отражённое имя класса: префикс `A` отброшен -> UOOriginManager.
    spawn("/Script/OpenUniverse.UOOriginManager")

    # Сохранить уровень.
    les.save_current_level()
    log("карта сохранена: " + MAP_PATH)


main()
