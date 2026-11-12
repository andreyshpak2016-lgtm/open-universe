// Copyright Open Universe. Все права защищены.

#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Actor.h"
#include "OUBuildBlock.generated.h"

class UStaticMeshComponent;

/**
 * Строительный блок Open Universe.
 *
 * Базовый «кубик» конструктора: игрок ставит его из меню строительства.
 * Позже станет частью воксельной сетки (как в `ou-build`), сейчас — простая
 * видимая деталь с цветом по типу материала.
 */
UCLASS()
class OPENUNIVERSE_API AOUBuildBlock : public AActor
{
	GENERATED_BODY()

public:
	AOUBuildBlock();

	/** Размер блока, см (куб). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Build")
	float BlockSize = 200.f;

	/** Цвет блока (задаётся меню строительства). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Build")
	FLinearColor BlockColor = FLinearColor(0.4f, 0.6f, 0.9f);

	/** Применить размер/цвет к мешу (вызывается меню после спавна). */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Build")
	void ApplyLook();

protected:
	virtual void BeginPlay() override;

	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Build")
	UStaticMeshComponent* BlockMesh = nullptr;
};
