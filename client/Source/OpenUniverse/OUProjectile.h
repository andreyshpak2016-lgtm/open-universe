// Copyright Open Universe. Все права защищены.

#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Actor.h"
#include "OUProjectile.generated.h"

class UStaticMeshComponent;
class UProjectileMovementComponent;

/**
 * Снаряд (трассер) ручного оружия персонажа.
 *
 * Простой видимый «болт»: светящаяся сфера, летящая по прямой с заданной
 * скоростью и исчезающая по времени жизни. Физику UE не используем — полёт
 * считаем сами (как и всё в открытом мире), чтобы поведение было предсказуемым
 * и дешёвым.
 */
UCLASS()
class OPENUNIVERSE_API AOUProjectile : public AActor
{
	GENERATED_BODY()

public:
	AOUProjectile();

	/** Скорость полёта, см/с. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Weapon")
	float Speed = 4500.f;

	/** Время жизни, с (после — снаряд исчезает). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Weapon")
	float LifeSeconds = 2.0f;

	/** Запустить снаряд из точки с направлением. */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Weapon")
	void Launch(const FVector& Direction);

protected:
	virtual void BeginPlay() override;
	virtual void Tick(float DeltaSeconds) override;

	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Weapon")
	UStaticMeshComponent* BoltMesh = nullptr;

private:
	/** Направление полёта (единичное). */
	FVector Dir = FVector::ForwardVector;

	/** Сколько уже летим, с. */
	float Age = 0.f;

	/** Уже запущен (защита от повторного Launch). */
	bool bLaunched = false;
};
