// Copyright Open Universe. Все права защищены.

#pragma once

#include "CoreMinimal.h"
#include "Kismet/BlueprintFunctionLibrary.h"
#include "OULandscapeBuilder.generated.h"

class AActor;
class UMaterialInterface;

/** Высоты ландшафта (мировой Z на вершину, построчно) — для проверок. */
USTRUCT(BlueprintType)
struct FOULandscapeHeights
{
	GENERATED_BODY()

	/** Мировая высота Z всех вершин, `Heights[Y * SizeX + X]`, в см. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Landscape")
	TArray<float> Heights;

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Landscape")
	int32 SizeX = 0;

	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Landscape")
	int32 SizeY = 0;
};

/**
 * Мост «Python → UE Landscape» (только в редакторе).
 *
 * ## Зачем это нужно
 * В UE 5.5 **нет Python-API**, создающего настоящий `ALandscape` с компонентами:
 * редакторная фабрика отдаёт пустышку `ALandscapePlaceholder`, а наполнение
 * хитмапом делает C++-метод `ALandscapeProxy::Import(...)`, который в Python не
 * отражён. Поэтому здесь — единственный недостающий кусок: принять массив высот
 * и построить из него реальный ландшафт.
 *
 * ## Как это работает
 * Функция повторяет то, что делает редактор при «New Landscape»:
 * создаёт `ALandscape`, задаёт масштаб и материал, кодирует высоты в
 * `uint16`-формат движка (см. `LandscapeDataAccess`) и вызывает `Import`.
 *
 * ## Точность координат
 * UE хранит высоту вершин как `uint16` с шагом 1/128 «локальной» единицы, а
 * мировая высота = локальная × масштаб.Z актора. Поэтому одна и та же карта
 * высот при разном масштабе даёт разный рельеф — вызывающий сам задаёт масштаб.
 *
 * ## Запуск
 * Скрипты Python выполняются **в полной сессии редактора**
 * (`UnrealEditor.exe <проект> -ExecutePythonScript=...`). В commandlet-режиме
 * создание ландшафта движок не поддерживает.
 *
 * ## Почему `AActor*`, а не `ALandscape*`
 * Типы `ALandscape`/`ALandscapeProxy` доступны только редактору. UHT добавляет
 * ссылки на модуль `Landscape` в сгенерированный код **даже для игровой
 * сборки**, поэтому в подписях стоит базовый `AActor*` — модуль Landscape
 * подключается только редактору.
 */
UCLASS()
class OPENUNIVERSE_API UOULandscapeBuilder : public UBlueprintFunctionLibrary
{
	GENERATED_BODY()

public:
#if WITH_EDITOR
	/**
	 * Построить ландшафт из карты высот (только в редакторе).
	 *
	 * @param Heights        Нормализованные высоты в диапазоне [0..1], построчно:
	 *                       `Heights[Y * SizeX + X]`, всего `SizeX * SizeY` значений.
	 * @param SizeX          Ширина карты в вершинах (`>= 2`).
	 * @param SizeY          Высота карты в вершинах (`>= 2`).
	 * @param Location       Мировая позиция актора ландшафта (см).
	 * @param Scale          Масштаб актора (см на «квад»); Z задаёт размах рельефа.
	 * @param HeightRange    Полный размах рельефа по вертикали, см (от −range/2 до +range/2).
	 * @param Material       Материал ландшафта (может быть `None` — тогда серый по умолчанию).
	 * @param QuadsPerSection Квадов в одной секции (например, 63).
	 * @param SectionsPerComponent Секций в компоненте (например, 1).
	 * @return Созданный ландшафт (как `AActor*`) или `None` при ошибке (причина — в логе).
	 */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Landscape")
	static AActor* BuildLandscapeFromHeightmap(
		const TArray<float>& Heights,
		int32 SizeX,
		int32 SizeY,
		const FVector& Location,
		const FVector& Scale,
		float HeightRange,
		UMaterialInterface* Material,
		int32 QuadsPerSection,
		int32 SectionsPerComponent);

	/**
	 * Прочитать мировые высоты Z всех вершин ландшафта (обратно из данных).
	 *
	 * Нужно для проверок без окна: сравнить, что построено, с тем, что задавали.
	 * Возвращает `false`, если ландшафт невалиден.
	 *
	 * @param Landscape Ландшафт (тот, что вернул `BuildLandscapeFromHeightmap`).
	 * @param Out       (выход) высоты в мировых Z (см), построчно.
	 */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Landscape")
	static bool ReadLandscapeHeights(AActor* Landscape, FOULandscapeHeights& Out);
#endif // WITH_EDITOR
};
