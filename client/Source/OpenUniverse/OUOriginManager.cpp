// Copyright Open Universe. Все права защищены.

#include "OUOriginManager.h"
#include "EngineUtils.h" // TActorIterator
#include "GameFramework/Actor.h"

DEFINE_LOG_CATEGORY_STATIC(LogOUOrigin, Log, All);

AUOOriginManager::AUOOriginManager()
{
	PrimaryActorTick.bCanEverTick = false;
}

FVector AUOOriginManager::GetWorldPosition(const AActor* Actor) const
{
	if (!Actor)
	{
		return Origin;
	}
	// Мировая позиция = локальная в уровне + текущее начало.
	return Actor->GetActorLocation() + Origin;
}

void AUOOriginManager::SetWorldPosition(AActor* Actor, const FVector& WorldPosition)
{
	if (!Actor)
	{
		return;
	}
	// Перевод мировой позиции в локальную для рендера.
	Actor->SetActorLocation(WorldPosition - Origin);
}

void AUOOriginManager::UpdateOrigin(const AActor* Focus)
{
	if (!Focus)
	{
		return;
	}
	const FVector Local = Focus->GetActorLocation();
	// Если актор ушёл слишком далеко от нуля — переносим начало на него.
	if (Local.SizeSquared() > ShiftThreshold * ShiftThreshold)
	{
		RebaseTo(Local + Origin);
	}
}

void AUOOriginManager::RebaseTo(const FVector& WorldPosition)
{
	// Новая дельта: насколько сдвигается начало.
	const FVector Delta = WorldPosition - Origin;
	Origin = WorldPosition;
	ApplyShift(Delta);
	OnOriginShifted.Broadcast(Delta, this);
	UE_LOG(LogOUOrigin, Verbose, TEXT("Floating origin: сдвиг на %s"), *Delta.ToString());
}

void AUOOriginManager::ApplyShift(const FVector& Delta)
{
	// Сдвигаем все акторы мира на ту же дельту, но в противоположную сторону,
	// поэтому визуально сцена остаётся на месте.
	UWorld* World = GetWorld();
	if (!World)
	{
		return;
	}
	for (TActorIterator<AActor> It(World); It; ++It)
	{
		AActor* Actor = *It;
		if (Actor && Actor != this)
		{
			Actor->AddActorWorldOffset(-Delta, false);
		}
	}
}
