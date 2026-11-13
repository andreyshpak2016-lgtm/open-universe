// Copyright Open Universe. Все права защищены.

#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Pawn.h"
#include "OUCharacterPawn.generated.h"

class UCapsuleComponent;
class UCameraComponent;
class UStaticMeshComponent;
class USceneComponent;
class UMaterialInterface;
class USkeletalMeshComponent;
class UPointLightComponent;
class AOUShipPawn;
class AOUProjectile;
class AOUBuildBlock;

/** Тип блока в палитре строительства. */
UENUM(BlueprintType)
enum class EOUBuildPart : uint8
{
	Hull    UMETA(DisplayName = "Корпус"),
	Panel   UMETA(DisplayName = "Панель"),
	Beam    UMETA(DisplayName = "Балка"),
	Light   UMETA(DisplayName = "Свет"),
	Count   UMETA(Hidden)
};

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

	/** Выстрелить из ручного оружия (трассер + вспышка). */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Weapon")
	void FireWeapon();

	// --- Меню строительства ---

	/** Открыто ли меню строительства. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Build")
	bool bBuildMenuOpen = false;

	/** Выбранный тип блока в палитре. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Build")
	int32 BuildSelection = 0;

	/** Сколько блоков поставлено (для меню строительства). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Build")
	int32 BuiltCount = 0;

	/** Открыть/закрыть меню строительства (клавиша B). */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Build")
	void ToggleBuildMenu();

	/** Выбрать блок в палитре (стрелки/клик). */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Build")
	void SelectBuildPart(int32 Index);

	/** Поставить выбранный блок перед персонажем. */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Build")
	void PlaceBuildBlock();

	/** Удалить блок, на который смотрит персонаж (сервер-авторитетно). */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Build")
	void RemoveBuildBlock();

	/** Перекрасить блок, на который смотрит персонаж, в выбранный цвет. */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Build")
	void PaintBuildBlock();

	/** Применить авторитетные правки конструкций, пришедшие от сервера. */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Build")
	void ApplyServerBuildEdits();

	/** Мировая позиция центра вокселя по его координате в сетке конструкции. */
	FVector VoxelWorld(const FVector& ServerCoord) const;

	/** Сетевой клиент (для отправки команд строительства). */
	UPROPERTY(BlueprintReadWrite, Category = "Open Universe|Build")
	class UOUClientNet* Net = nullptr;

	/** Название типа блока (для меню). */
	static FString BuildPartName(int32 Index);

	/** Цвет типа блока (для меню и самого блока). */
	static FLinearColor BuildPartColor(int32 Index);

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

	/** Выбор следующего/предыдущего блока в палитре. */
	void OnBuildNext();
	void OnBuildPrev();

	/** Удаление/покраска по клавишам (правый клик / средний клик). */
	void OnBuildRemove();
	void OnBuildPaint();

	/**
	 * Куда смотрит персонаж в сетке конструкции: координата вокселя под прицелом
	 * и точка попадания. Возвращает false, если впереди нет блока.
	 */
	bool TraceBuildTarget(FIntVector& OutCoord, FVector& OutHit, FVector& OutNormal) const;

	/** Мировая точка → координата вокселя сетки (обратное к `VoxelWorld`). */
	FIntVector WorldToVoxel(const FVector& WorldPos) const;

	/**
	 * Мировое направление → сдвиг на соседнюю ячейку сетки по доминирующей оси.
	 * Нужно, чтобы ставить блок в грань, на которую смотрит игрок.
	 */
	FIntVector WorldDirToVoxelShift(const FVector& WorldDir) const;

	/** Найти сетевой компонент (на корабле) для отправки команд строительства. */
	void ResolveNet();

	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Character")
	UCapsuleComponent* Capsule = nullptr;

	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Character")
	UCameraComponent* Camera = nullptr;

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

	// --- Ручное оружие ---
	// Оружие висит на правом плече (в руке) и смотрит вперёд по курсу.

	/** Корпус оружия (в правой руке). */
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Weapon")
	UStaticMeshComponent* GunMesh = nullptr;

	/** Вспышка у дула (точка вылета) — включается на кадр выстрела. */
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Weapon")
	UPointLightComponent* MuzzleFlash = nullptr;

	/** Секунд между выстрелами. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Weapon")
	float FireCooldown = 0.16f;

	/** Сколько держится вспышка, с. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Weapon")
	float MuzzleFlashTime = 0.05f;

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

	/** Осталось времени вспышки, с. */
	float MuzzleTimer = 0.f;

	/** Время до следующего выстрела, с. */
	float FireTimer = 0.f;

	/** Можно ли стрелять (зажимание кнопки огня). */
	bool bFireHeld = false;

	/** Тестовый автоогонь (ключ `-OUAutoFire`): стреляем сами. */
	bool bAutoFire = false;

	/** Указатель на последний снаряд (для визуальной цепочки). */
	TWeakObjectPtr<AOUProjectile> LastBolt;

	/** Тестовое строительство (ключ `-OUAutoBuild`): поставить блок сами. */
	bool bAutoBuild = false;

	/** Автотест уже покрасил и удалил блок (делаем по одному разу). */
	bool bAutoTestedPaint = false;
	bool bAutoTestedRemove = false;

	/** Координаты блоков, поставленных в этом сеансе (для автотеста). */
	TArray<FIntVector> BuiltCoords;

	/** Таймер авто-строительства, с. */
	float BuildTimer = 0.f;

	/** Воксели, уже применённые с сервера: ключ «(x,y,z)» → актор блока. */
	TMap<FIntVector, TObjectPtr<AOUBuildBlock>> ServerBlocks;

	/** Обратный указатель: актор блока → его координата в сетке. Нужен, чтобы
	 *  точно адресовать удаление/покраску по блоку, на который смотрит игрок
	 *  (пересчёт по мировой точке даёт ошибку на границе ячеек). */
	TMap<TWeakObjectPtr<AActor>, FIntVector> BlockCoords;

	/** Начало отсчёта сетки конструкции (в серверных единицах). */
	FVector BuildOriginServer = FVector::ZeroVector;

	/** Крайняя применённая ревизия правок (для подтверждения серверу). */
	int64 LastAppliedRevision = 0;
};
