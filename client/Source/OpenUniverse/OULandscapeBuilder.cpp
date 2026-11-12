// Copyright Open Universe. Все права защищены.

#include "OULandscapeBuilder.h"

#if WITH_EDITOR

#include "Landscape.h"
#include "LandscapeProxy.h"
#include "LandscapeInfo.h"
#include "LandscapeDataAccess.h"
#include "LandscapeEdit.h"
#include "LandscapeComponent.h"
#include "Materials/MaterialInterface.h"
#include "Engine/World.h"
#include "Engine/Level.h"
#include "GameFramework/Actor.h"

DEFINE_LOG_CATEGORY_STATIC(LogOULandscape, Log, All);

AActor* UOULandscapeBuilder::BuildLandscapeFromHeightmap(
	const TArray<float>& Heights,
	int32 SizeX,
	int32 SizeY,
	const FVector& Location,
	const FVector& Scale,
	float HeightRange,
	UMaterialInterface* Material,
	int32 QuadsPerSection,
	int32 SectionsPerComponent)
{
	// --- Проверка входных данных ---
	if (SizeX < 2 || SizeY < 2)
	{
		UE_LOG(LogOULandscape, Error, TEXT("Нужно минимум 2x2 вершины, получено %dx%d"), SizeX, SizeY);
		return nullptr;
	}
	if (Heights.Num() != SizeX * SizeY)
	{
		UE_LOG(LogOULandscape, Error, TEXT("Размер карты высот %d не совпадает с %dx%d=%d"),
			Heights.Num(), SizeX, SizeY, SizeX * SizeY);
		return nullptr;
	}
	if (QuadsPerSection < 1 || SectionsPerComponent < 1)
	{
		UE_LOG(LogOULandscape, Error, TEXT("Некорректная нарезка: квадов в секции %d, секций %d"),
			QuadsPerSection, SectionsPerComponent);
		return nullptr;
	}

	UWorld* World = GEditor ? GEditor->GetEditorWorldContext().World() : nullptr;
	if (!World)
	{
		UE_LOG(LogOULandscape, Error, TEXT("Нет редакторного мира — ландшафт не создать"));
		return nullptr;
	}

	const int32 ComponentSizeQuads = QuadsPerSection * SectionsPerComponent;
	if ((SizeX - 1) % ComponentSizeQuads != 0 || (SizeY - 1) % ComponentSizeQuads != 0)
	{
		UE_LOG(LogOULandscape, Error,
			TEXT("Размер %dx%d не кратен компоненту %d: сетка не выровнена"),
			SizeX, SizeY, ComponentSizeQuads);
		return nullptr;
	}

	const int32 NumComponentsX = (SizeX - 1) / ComponentSizeQuads;
	const int32 NumComponentsY = (SizeY - 1) / ComponentSizeQuads;

	// --- Создаём актор ландшафта ---
	FActorSpawnParameters SpawnParams;
	SpawnParams.SpawnCollisionHandlingOverride = ESpawnActorCollisionHandlingMethod::AlwaysSpawn;

	ALandscape* Landscape = World->SpawnActor<ALandscape>(
		ALandscape::StaticClass(), Location, FRotator::ZeroRotator, SpawnParams);
	if (!Landscape)
	{
		UE_LOG(LogOULandscape, Error, TEXT("Не удалось создать актор ландшафта"));
		return nullptr;
	}

	// Без edit-layers: проще и не требует редакторных слоёв.
	Landscape->bCanHaveLayersContent = false;
	if (Material)
	{
		Landscape->LandscapeMaterial = Material;
	}
	// Масштаб задаёт размер квада (см) и, по Z, размах рельефа.
	Landscape->SetActorScale3D(Scale);
	FString Label = FString::Printf(TEXT("OULandscape_%dx%d"), SizeX, SizeY);
	Landscape->SetActorLabel(Label);

	// --- Готовим данные высот в формате движка ---
	// Локальная высота = мировая / масштаб.Z; кодируется как uint16:
	//   value = localHeight * 128 + 32768   (см. LandscapeDataAccess).
	//
	// Нормализованная высота h в [0..1] превращается в мировой Z:
	//   worldZ = (h - 0.5) * HeightRange,
	// затем в локальную и в uint16.
	const float ZScale = FMath::Max(FMath::Abs(Scale.Z), KINDA_SMALL_NUMBER);
	const float LocalHalf = HeightRange * 0.5f / ZScale;    // полуразмах в локальных единицах

	TArray<uint16> HeightData;
	HeightData.SetNumUninitialized(SizeX * SizeY);
	for (int32 Y = 0; Y < SizeY; ++Y)
	{
		for (int32 X = 0; X < SizeX; ++X)
		{
			const float H = FMath::Clamp(Heights[Y * SizeX + X], 0.f, 1.f);
			const float LocalH = (H - 0.5f) * 2.f * LocalHalf;   // [-LocalHalf, +LocalHalf]
			const float Encoded = LocalH * LANDSCAPE_INV_ZSCALE + LandscapeDataAccess::MidValue;
			HeightData[Y * SizeX + X] = static_cast<uint16>(
				FMath::Clamp(FMath::RoundToInt(Encoded), 0, static_cast<int32>(LandscapeDataAccess::MaxValue)));
		}
	}

	TMap<FGuid, TArray<uint16>> HeightDataPerLayers;
	HeightDataPerLayers.Add(FGuid(), MoveTemp(HeightData));

	TMap<FGuid, TArray<FLandscapeImportLayerInfo>> MaterialLayerDataPerLayers;
	MaterialLayerDataPerLayers.Add(FGuid(), TArray<FLandscapeImportLayerInfo>());

	// --- Импорт (собственно постройка компонентов и текстур) ---
	Landscape->Import(
		FGuid::NewGuid(),
		0, 0, SizeX - 1, SizeY - 1,
		SectionsPerComponent, QuadsPerSection,
		HeightDataPerLayers,
		TEXT(""),
		MaterialLayerDataPerLayers,
		ELandscapeImportAlphamapType::Additive,
		TArrayView<const FLandscapeLayer>());

	// Финализация: коллизии, инфо-объект, регистрация компонентов.
	if (ULandscapeInfo* Info = Landscape->GetLandscapeInfo())
	{
		Info->RecreateCollisionComponents();
		Info->UpdateAllAddCollisions();
	}
	Landscape->ReregisterAllComponents();
	Landscape->MarkPackageDirty();

	UE_LOG(LogOULandscape, Log,
		TEXT("Ландшафт построен: %dx%d вершин, компонентов %dx%d, квад %d, секций %d"),
		SizeX, SizeY, NumComponentsX, NumComponentsY, QuadsPerSection, SectionsPerComponent);

	return Landscape;
}

bool UOULandscapeBuilder::ReadLandscapeHeights(AActor* LandscapeActor, FOULandscapeHeights& Out)
{
	Out.Heights.Reset();
	Out.SizeX = 0;
	Out.SizeY = 0;

	ALandscapeProxy* Landscape = Cast<ALandscapeProxy>(LandscapeActor);
	if (!Landscape)
	{
		UE_LOG(LogOULandscape, Error, TEXT("ReadLandscapeHeights: ландшафт не задан"));
		return false;
	}
	ULandscapeInfo* Info = Landscape->GetLandscapeInfo();
	if (!Info)
	{
		UE_LOG(LogOULandscape, Error, TEXT("ReadLandscapeHeights: нет LandscapeInfo"));
		return false;
	}

	int32 MinX = 0, MinY = 0, MaxX = 0, MaxY = 0;
	if (!Info->GetLandscapeExtent(MinX, MinY, MaxX, MaxY))
	{
		UE_LOG(LogOULandscape, Error, TEXT("ReadLandscapeHeights: не удалось получить границы"));
		return false;
	}

	const int32 SizeX = MaxX - MinX + 1;
	const int32 SizeY = MaxY - MinY + 1;

	TArray<uint16> Raw;
	Raw.SetNumUninitialized(SizeX * SizeY);

	FLandscapeEditDataInterface EditInterface(Info, /*bInUploadTextureChangesToGPU=*/false);
	EditInterface.GetHeightData(MinX, MinY, MaxX, MaxY, Raw.GetData(), 0);

	// Локальная высота → мировая: умножаем на масштаб Z актора.
	const float ZScale = Landscape->GetActorScale3D().Z;
	Out.Heights.SetNumUninitialized(SizeX * SizeY);
	for (int32 I = 0; I < Raw.Num(); ++I)
	{
		Out.Heights[I] = LandscapeDataAccess::GetLocalHeight(Raw[I]) * ZScale;
	}
	Out.SizeX = SizeX;
	Out.SizeY = SizeY;
	return true;
}

#endif // WITH_EDITOR
