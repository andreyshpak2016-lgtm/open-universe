// Copyright Open Universe. Все права защищены.

#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Pawn.h"
#include "OUCharacterPawn.generated.h"

class UCapsuleComponent;
class UCameraComponent;
class UStaticMeshComponent;
class USceneComponent;
class USpringArmComponent;
class UMaterialInterface;
class USkeletalMeshComponent;
class AOUShipPawn;

/**
 * Персонаж игрока (ходьба по поверхности планеты).
 *
 * Появляется на поверхности планеты (игрок спавнится пешком), корабль стоит
 * рядом. Передвигается по сферической поверхности: «низ» — к центру планеты,
 * поэтому обычный CharacterMovementComponent (он считает верх глобальным +Z)
 * не подходит — гравитацию и привязку к рельефу считаем сами, той же формулой,
 * что сервер.
 *
 * Фигурка **процедурная**: торс, голова, руки и ноги собраны из базовых мешей
 * движка. При ходьбе/беге проигрывается простая анимация (махи руками и
 * ногами, покачивание корпуса) — без внешних ассетов и скелета.
 */
UCLASS()
class OPENUNIVERSE_API AOUCharacterPawn : public APawn
{
	GENERATED_BODY()

public:
	AOUCharacterPawn();

	/** Корабль, рядом с которым стоит персонаж (для возврата в кабину). */
	UPROPERTY(BlueprintReadWrite, Category = "Open Universe|Character")
	AOUShipPawn* Ship = nullptr;

	/** Радиус капсулы персонажа, см. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Character")
	float CapsuleRadius = 40.f;

	/** Полувысота капсулы, см. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Character")
	float CapsuleHalfHeight = 90.f;

	/** Скорость ходьбы, см/с. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Character")
	float WalkSpeed = 900.f;

	/** Ускорение свободного падения у поверхности, см/с². */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Character")
	float Gravity = 1800.f;

	/** Скорость прыжка, см/с. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Character")
	float JumpSpeed = 1200.f;

	/** Амплитуда маха конечностей на бегу, градусы. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Character")
	float StrideAmplitude = 45.f;

	/** Частота шагов на полной скорости, шагов/с. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Character")
	float StrideFrequency = 1.5f;

	/** Текущая скорость, см/с (в осях UE). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Character")
	FVector Velocity = FVector::ZeroVector;

	/** Угол взгляда (курс/тангаж), рад. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Character")
	float ViewYaw = 0.f;
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Character")
	float ViewPitch = 0.f;

	/** Стоит ли персонаж на поверхности (для прыжка). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Character")
	bool bGrounded = false;

	/** Поставить персонажа в точку (при выходе из корабля). */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Character")
	void PlaceAt(const FVector& WorldLocation, float LookYaw);

protected:
	virtual void BeginPlay() override;
	virtual void Tick(float DeltaSeconds) override;
	virtual void SetupPlayerInputComponent(class UInputComponent* PlayerInputComponent) override;

	/** Радиус поверхности планеты в направлении (UE-оси), см. */
	float SurfaceRadius(const FVector& DirUE) const;

	/**
	 * Проиграть цикл ходьбы/бега: махи руками и ногами, покачивание корпуса.
	 * @param DeltaSeconds  шаг времени
	 * @param HorizontalSpeed  горизонтальная скорость, см/с
	 */
	void AnimateLocomotion(float DeltaSeconds, float HorizontalSpeed);

	void AxisForward(float Value);
	void AxisRight(float Value);
	void AxisLookYaw(float Value);
	void AxisLookPitch(float Value);
	void OnJumpPressed();
	void OnJumpReleased();

	/** Зажатие F: копим время и возвращаемся в корабль по порогу. */
	void OnExitPressed();
	void OnExitReleased();

	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Character")
	UCapsuleComponent* Capsule = nullptr;

	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Character")
	UCameraComponent* Camera = nullptr;

	/** Штанга камеры: даёт вид от третьего лица (видно персонажа и корабль). */
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Character")
	USpringArmComponent* SpringArm = nullptr;

	/** Торс (корпус фигурки). */
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Character")
	UStaticMeshComponent* BodyMesh = nullptr;

	/** Голова. */
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Character")
	UStaticMeshComponent* HeadMesh = nullptr;

	// --- Конечности ---
	// Каждая конечность — «шарнир» (USceneComponent) в точке крепления и меш,
	// подвешенный под ним. Анимация вращает шарнир, поэтому конечность качается
	// из плеча/бедра, а не вокруг собственного центра.

	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Character")
	USceneComponent* ShoulderL = nullptr;
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Character")
	USceneComponent* ShoulderR = nullptr;
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Character")
	USceneComponent* HipL = nullptr;
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Character")
	USceneComponent* HipR = nullptr;

	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Character")
	UStaticMeshComponent* ArmL = nullptr;
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Character")
	UStaticMeshComponent* ArmR = nullptr;
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Character")
	UStaticMeshComponent* LegL = nullptr;
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Character")
	UStaticMeshComponent* LegR = nullptr;

private:
	float InputForward = 0.f;
	float InputRight = 0.f;
	float InputLookYaw = 0.f;
	float InputLookPitch = 0.f;
	bool bJumpQueued = false;

	/** Зажатие F для возврата в корабль. */
	bool bExitHeld = false;
	float ExitHoldTime = 0.f;

	/** Фаза цикла шага, рад (растёт при движении по земле). */
	float StridePhase = 0.f;
	/** Сглаженная «скорость бега» [0..1] — для плавного перехода анимации. */
	float RunBlend = 0.f;

	/** Тестовый автоход (ключ `-OUAutoWalk`): персонаж идёт вперёд сам. */
	bool bAutoWalk = false;
};
