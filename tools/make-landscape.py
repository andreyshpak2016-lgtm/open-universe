# -*- coding: utf-8 -*-
"""
Генератор ландшафта Open Universe: numpy -> C++-мост -> UE Landscape.

Строит карту высот детерминированным value-noise (тот же приём, что и на
сервере), затем через `OULandscapeBuilder` создаёт настоящий UE Landscape.

Запуск (полная сессия редактора, НЕ commandlet):
  UnrealEditor.exe <проект> -ExecutePythonScript="tools/make-landscape.py"

Опции (через переменные окружения, необязательно):
  OU_LAND_SIZE       число вершин по стороне (по умолчанию 8129)
  OU_LAND_MAP        куда сохранить карту (по умолчанию /Game/Maps/OpenUniverse)
  OU_LAND_SAVE       1 — сохранить карту (по умолчанию 1)
"""

import io
import math
import os
import traceback

import unreal

try:
    import numpy as np
except Exception:
    np = None

REPORT = "C:/Users/fisk-/AppData/Local/Temp/opencode/land_result.txt"
LOG = []


def log(msg):
    LOG.append(str(msg))
    try:
        unreal.log("[OpenUniverse/Landscape] " + str(msg))
    except Exception:
        pass
    try:
        with io.open(REPORT, "w", encoding="utf-8") as f:
            f.write("\n".join(LOG))
    except Exception:
        pass


def lattice_hash(x, y, seed):
    """Хеш решётки в [0,1) — тот же приём, что в OUDeterministic.h."""
    h = int(seed) & 0xFFFFFFFFFFFFFFFF
    for c in (x, y):
        h ^= (int(c) * 0x9E3779B97F4A7C15) & 0xFFFFFFFFFFFFFFFF
        h = (h ^ (h >> 30)) * 0xBF58476D1CE4E5B9 & 0xFFFFFFFFFFFFFFFF
        h = (h ^ (h >> 27)) * 0x94D049BB133111EB & 0xFFFFFFFFFFFFFFFF
    return ((h >> 11) & ((1 << 53) - 1)) / float(1 << 53)


def value_noise(u, v, seed):
    """Кубическая интерполяция value-noise на решётке."""
    ix, iy = math.floor(u), math.floor(v)
    fx, fy = u - ix, v - iy
    sx, sy = fx * fx * (3 - 2 * fx), fy * fy * (3 - 2 * fy)
    c = lambda dx, dy: lattice_hash(ix + dx, iy + dy, seed)
    a = c(0, 0) + (c(1, 0) - c(0, 0)) * sx
    b = c(0, 1) + (c(1, 1) - c(0, 1)) * sx
    return a + (b - a) * sy


def make_heightmap(size, seed=0xC0FFEE):
    """Карта высот [0..1] из нескольких октав value-noise."""
    if np is None:
        raise RuntimeError("numpy недоступен в Python редактора")
    xs = np.arange(size, dtype=np.float64)
    yy, xx = np.meshgrid(xs, xs, indexing="ij")

    def value_noise(u, v):
        ix, iy = np.floor(u), np.floor(v)
        fx, fy = u - ix, v - iy
        # Квинтическая интерполяция — глаже кубической, без «клеток» решётки.
        sx = fx * fx * fx * (fx * (fx * 6 - 15) + 10)
        sy = fy * fy * fy * (fy * (fy * 6 - 15) + 10)

        def h(dx, dy):
            hx = (ix + dx).astype(np.int64)
            hy = (iy + dy).astype(np.int64)
            hh = np.full(hx.shape, seed & 0xFFFFFFFFFFFFFFFF, dtype=np.uint64)
            for arr in (hx, hy):
                hh = hh ^ (arr.astype(np.uint64) * np.uint64(0x9E3779B97F4A7C15))
                hh = (hh ^ (hh >> np.uint64(30))) * np.uint64(0xBF58476D1CE4E5B9)
                hh = (hh ^ (hh >> np.uint64(27))) * np.uint64(0x94D049BB133111EB)
            return ((hh >> np.uint64(11)) & np.uint64((1 << 53) - 1)).astype(np.float64) / float(1 << 53)

        a = h(0, 0) + (h(1, 0) - h(0, 0)) * sx
        b = h(0, 1) + (h(1, 1) - h(0, 1)) * sx
        return a + (b - a) * sy

    # Плоское «плато» крупного масштаба + хребтовый шум для гор.
    # Domain warp: смещаем координаты другим шумом — так рельеф перестаёт быть
    # «ватным» и появляются извилистые хребты и долины. Warp — тоже fBm
    # (несколько октав), иначе видна «клетка» решётки.
    warp = np.zeros((size, size), dtype=np.float64)
    wamp, wfreq, wn = 1.0, 1.0 / 600.0, 0.0
    for _ in range(3):
        warp += value_noise(xx * wfreq, yy * wfreq) * wamp
        wn += wamp
        wamp *= 0.5
        wfreq *= 2.2
    warp = (warp / wn - 0.5) * 220.0
    base = np.zeros((size, size), dtype=np.float64)
    ridge = np.zeros((size, size), dtype=np.float64)
    amp, freq, wsum = 1.0, 1.0 / 180.0, 0.0
    for octave in range(8):
        # Поворот решётки на каждой октаве: иначе квадратная сетка value-noise
        # даёт заметную «клетку» на рельефе.
        ang = octave * 0.7
        ca, sa = math.cos(ang), math.sin(ang)
        px = (xx + warp) * freq
        py = (yy + warp) * freq
        u = px * ca - py * sa
        v = px * sa + py * ca
        n = value_noise(u, v)
        base += n * amp
        # Хребты: 1 - |2n-1| даёт острые линии; возводим в степень для резкости.
        ridge += (1.0 - np.abs(2.0 * n - 1.0)) ** 2.2 * amp
        wsum += amp
        amp *= 0.52
        freq *= 2.0

    base /= wsum
    ridge /= wsum

    # Континентальная маска: где «материк», там горы, где океан — низины.
    cont = value_noise(xx * (1.0 / 260.0) + 100.0, yy * (1.0 / 260.0) + 100.0)
    cont = np.clip((cont - 0.35) / 0.45, 0.0, 1.0)
    height = base * 0.45 + ridge * cont * 0.65
    # Нормируем в [0..1].
    height = (height - height.min()) / max(height.max() - height.min(), 1e-9)
    return height


def ensure_scene(size, scale_cm):
    """Солнце, небо и точка старта — чтобы демо-карту было видно."""
    eas = unreal.get_editor_subsystem(unreal.EditorActorSubsystem)
    names = [a.get_class().get_name() for a in eas.get_all_level_actors()]
    half = (size - 1) * scale_cm * 0.5

    def spawn(path, loc, rot=None):
        cls = unreal.load_class(None, path)
        if cls is None:
            return None
        r = rot if rot is not None else unreal.Rotator(0.0, 0.0, 0.0)
        return eas.spawn_actor_from_class(cls, loc, r)

    if "SkyAtmosphere" not in names:
        spawn("/Script/Engine.SkyAtmosphere", unreal.Vector(0, 0, 0))
    if "DirectionalLight" not in names:
        sun = spawn("/Script/Engine.DirectionalLight",
                    unreal.Vector(0, 0, half),
                    unreal.Rotator(-45.0, -35.0, 0.0))
        if sun:
            comp = sun.get_editor_property("light_component")
            if comp:
                comp.set_editor_property("mobility", unreal.ComponentMobility.MOVABLE)
                comp.set_editor_property("intensity", 4.0)
    if "SkyLight" not in names:
        sky = spawn("/Script/Engine.SkyLight", unreal.Vector(0, 0, half))
        if sky:
            comp = sky.get_editor_property("light_component")
            if comp:
                comp.set_editor_property("mobility", unreal.ComponentMobility.MOVABLE)
                comp.set_editor_property("real_time_capture", True)
    if "PlayerStart" not in names:
        # Смотрим на рельеф сверху и под углом.
        spawn("/Script/Engine.PlayerStart",
              unreal.Vector(0.0, 0.0, half * 1.2),
              unreal.Rotator(-35.0, 0.0, 0.0))
    log("сцена подготовлена (солнце/небо/старт)")


def main():
    size = int(os.environ.get("OU_LAND_SIZE", "2017"))
    # Отдельная демо-карта: генерация не должна затирать игровую сцену.
    map_path = os.environ.get("OU_LAND_MAP", "/Game/Maps/OULandscapeDemo")
    do_save = os.environ.get("OU_LAND_SAVE", "1") == "1"

    log("старт: размер %d, карта %s" % (size, map_path))

    les = unreal.get_editor_subsystem(unreal.LevelEditorSubsystem)
    if unreal.EditorAssetLibrary.does_asset_exist(map_path):
        log("открываю существующую карту %s" % map_path)
        les.load_level(map_path)
    else:
        log("новая карта %s" % map_path)
        les.new_level(map_path)

    # Убираем прежние ландшафты, чтобы не копить дубликаты.
    eas = unreal.get_editor_subsystem(unreal.EditorActorSubsystem)
    removed = 0
    for actor in eas.get_all_level_actors():
        if isinstance(actor, unreal.Landscape):
            eas.destroy_actor(actor)
            removed += 1
            continue
        if actor.get_class().get_name() == "LandscapePlaceholder":
            eas.destroy_actor(actor)
            removed += 1
    log("удалено прежних ландшафтов: %d" % removed)

    log("генерация карты высот...")
    height = make_heightmap(size).astype(np.float64).ravel().tolist()
    log("карта готова: %d значений" % len(height))

    # Плоская карта: при квад=100 см размер 2017 вершин ~ 2 км по стороне.
    quads_per_section = 63
    sections_per_component = 1
    component_quads = quads_per_section * sections_per_component
    if (size - 1) % component_quads:
        raise RuntimeError("размер %d не кратен компоненту %d" % (size, component_quads))

    scale_cm = 100.0
    location = unreal.Vector(0.0, 0.0, 0.0)
    actor_scale = unreal.Vector(scale_cm, scale_cm, scale_cm)
    height_range = 12000.0   # 120 м от низа до верха рельефа

    mat = None
    try:
        mat = unreal.load_asset("/Game/Materials/M_Landscape")
    except Exception:
        mat = None

    log("строю ландшафт (%d x %d)..." % (size, size))
    land = unreal.OULandscapeBuilder.build_landscape_from_heightmap(
        height, size, size, location, actor_scale, height_range,
        mat, quads_per_section, sections_per_component)
    if land is None:
        raise RuntimeError("мост вернул None — смотри лог редактора")

    log("ландшафт создан: %s" % land.get_name())
    comps = land.get_components_by_class(unreal.LandscapeComponent)
    log("компонентов: %d" % len(comps))
    bounds = land.get_actor_bounds(False)
    log("габариты: origin=%s extent=%s" % (bounds[0], bounds[1]))

    ensure_scene(size, scale_cm)

    if do_save:
        les.save_current_level()
        log("карта сохранена")

    # --- Проверка БЕЗ окна: читаем высоты обратно и сравниваем с заданными ---
    # Out-параметр USTRUCT в Python приходит как результат вызова.
    result = unreal.OULandscapeBuilder.read_landscape_heights(land)
    heights = None
    ok = False
    if isinstance(result, (tuple, list)):
        for part in result:
            if isinstance(part, unreal.OULandscapeHeights):
                heights = part
            elif isinstance(part, bool):
                ok = part
    elif isinstance(result, unreal.OULandscapeHeights):
        heights = result
        ok = True

    if heights is not None and len(heights.heights) > 0:
        n = len(heights.heights)
        log("прочитано высот: %d (задано %d), сетка %dx%d"
            % (n, len(height), heights.size_x, heights.size_y))
        if n == len(height):
            errs = []
            step = max(1, n // 4096)
            for i in range(0, n, step):
                expect = (height[i] - 0.5) * height_range
                got = heights.heights[i]
                errs.append(abs(got - expect))
            max_err = max(errs) if errs else 0.0
            log("макс. расхождение высот: %.3f см (точек: %d)" % (max_err, len(errs)))
            log("ПРОВЕРКА: %s" % ("OK" if max_err < 50.0 else "НЕ ПРОШЛА"))
        else:
            log("ПРОВЕРКА: НЕ ПРОШЛА (размер не совпал)")

        # Пишем прочитанные высоты в raw-файл: системный Python построит по ним
        # превью рельефа (скриншот UE без окна невозможен).
        try:
            import struct
            raw_path = "C:/Users/fisk-/AppData/Local/Temp/opencode/land_heightmap.bin"
            with open(raw_path, "wb") as f:
                f.write(struct.pack("<ii", heights.size_x, heights.size_y))
                f.write(struct.pack("<%df" % n, *list(heights.heights)))
            log("карта высот записана: %s" % raw_path)
        except Exception as e:
            log("не удалось записать карту высот: %s" % e)
    else:
        log("ПРОВЕРКА: НЕ ПРОШЛА (высоты не прочитаны: %s)" % str(result)[:120])

    log("ГОТОВО")


try:
    main()
except Exception as e:
    log("ОШИБКА: %s" % e)
    log(traceback.format_exc())
finally:
    try:
        unreal.SystemLibrary.quit_editor()
    except Exception:
        pass
