// Copyright Open Universe. Все права защищены.

#include "OUShipPawn.h"
#include "OUClientNet.h"
#include "OUDeterministic.h"
#include "OUInterop.h"
#include "OUOriginManager.h"
#include "OUVoxelPlanet.h"
#include "OUCharacterPawn.h"
#include "OUPlayerMenuController.h"
#include "Components/StaticMeshComponent.h"
#include "Components/InputComponent.h"
#include "Camera/CameraComponent.h"
#include "Engine/StaticMesh.h"
#include "UObject/ConstructorHelpers.h"
#include "Kismet/GameplayStatics.h"
#include "GameFramework/PlayerController.h"
#include "DrawDebugHelpers.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"
#include "OUGameMode.h"

DEFINE_LOG_CATEGORY_STATIC(LogOUShip, Log, All);

namespace
{
	/** Радиус домашней планеты в серверных единицах (совпадает с сервером). */
	constexpr double OUPlanetRadius = 500000.0;
	/** Ускорение свободного падения у поверхности, ед/с² (как на сервере). */
	constexpr double OUSurfaceGravity = 18.0;
	/** До какого расстояния (в радиусах) действует гравитация. */
	constexpr double OUGravityRangeRadii = 30.0;

	/**
	 * Гравитационное ускорение в точке (серверные оси).
	 * Та же формула обратных квадратов, что в `ou_sim::gravity_at`.
	 */
	FVector GravityAt(const FVector& Pos)
	{
		const double Dist = Pos.Size();
		if (Dist < 1e-6)
		{
			return FVector::ZeroVector;
		}
		if (Dist > OUPlanetRadius * OUGravityRangeRadii)
		{
			return FVector::ZeroVector; // космос
		}
		const FVector Up = Pos / Dist;
		const double R = FMath::Max(Dist, OUPlanetRadius);
		const double Ratio = OUPlanetRadius / R;
		return Up * (-OUSurfaceGravity * Ratio * Ratio);
	}
}

FVector AOUShipPawn::GravityAtAll(const FVector& Pos) const
{
	// Домашняя планета — в начале координат (как на сервере).
	FVector G = GravityAt(Pos);
	// Соседние тела системы: гладкие сферы, гравитация по тем же обратным
	// квадратам. Позиции серверные (1 ед = 1 м), точка Pos — тоже серверная.
	if (const UOUClientNet* NetC = FindComponentByClass<UOUClientNet>())
	{
		for (const FOUSystemBody& B : NetC->SystemBodies)
		{
			// Пропускаем домашнюю планету (она уже учтена как GravityAt).
			if (B.Center.IsNearlyZero() && B.Radius > 400000.f)
			{
				continue;
			}
			if (B.Radius <= 0.f)
			{
				continue;
			}
			const FVector Rel = Pos - B.Center;   // серверные единицы
			const double Dist = Rel.Size();
			if (Dist < 1.0)
			{
				continue;
			}
			const FVector Up = Rel / Dist;
			const double R = FMath::Max<double>(Dist, B.Radius);
			const double Ratio = B.Radius / R;
			G += Up * (-18.0 * Ratio * Ratio);
		}
	}
	return G;
}

AOUShipPawn::AOUShipPawn()
{
	PrimaryActorTick.bCanEverTick = true;

	ShipMesh = CreateDefaultSubobject<UStaticMeshComponent>(TEXT("ShipMesh"));
	RootComponent = ShipMesh;

	// Видимый «корабль»: базовый куб движка. Настоящая блочная/воксельная
	// конструкция появится позже; сейчас важно, что игрок видит свой корабль.
	static ConstructorHelpers::FObjectFinder<UStaticMesh> ShipShape(
		TEXT("/Engine/BasicShapes/Cube.Cube"));
	if (ShipShape.Succeeded())
	{
		ShipMesh->SetStaticMesh(ShipShape.Object);
	}
	ShipMesh->SetRelativeScale3D(FVector(3.f, 1.f, 0.6f));

	// Материал корабля — тот же параметрический, что у базовых фигур: задаём
	// цвет, чтобы корабль читался на фоне поверхности.
	static ConstructorHelpers::FObjectFinder<UMaterialInterface> ShipMat(
		TEXT("/Engine/BasicShapes/BasicShapeMaterial.BasicShapeMaterial"));
	if (ShipMat.Succeeded())
	{
		ShipMesh->SetMaterial(0, ShipMat.Object);
	}

	// Физику UE **не используем** в открытом мире: кинематику считаем сами,
	// как на сервере. Это принципиально для детерминизма и античита.
	ShipMesh->SetSimulatePhysics(false);
	ShipMesh->SetCollisionEnabled(ECollisionEnabled::NoCollision);

	// Камера — ВИД ОТ ПЕРВОГО ЛИЦА: камера в кабине, впереди и чуть выше центра.
	// Корпус корабля владельцу не показываем (видно только «из кабины»).
	Camera = CreateDefaultSubobject<UCameraComponent>(TEXT("Camera"));
	Camera->SetupAttachment(RootComponent);
	Camera->SetRelativeLocation(FVector(180.f, 0.f, 60.f));
	Camera->bUsePawnControlRotation = false;
	Camera->SetFieldOfView(100.f);

	// Свой корпус владельцу не показываем — вид от первого лица.
	ShipMesh->SetOwnerNoSee(true);

	// Корабль по умолчанию НЕ перехватывает управление: игрок появляется
	// персонажем, а в корабль садится по F (тогда контроллер Possess'ит его).
	AutoPossessPlayer = EAutoReceiveInput::Disabled;
}

void AOUShipPawn::BeginPlay()
{
	Super::BeginPlay();

	State.Id = 1;
	// Точка появления — на домашней планете (совпадает с сервером).
	// Если корабль уже поставлен снаружи (его создал персонаж рядом), не
	// переносим его снова; иначе ставим на сушу у поверхности.
	if (FParse::Param(FCommandLine::Get(), TEXT("OUMoonOrbit")))
	{
		// Диагностика: корабль над луной — видно её процедурный рельеф.
		// Луна (тело id 2) в (+560000,0,0) серверных единиц, радиус 60000.
		const FVector MoonCenterUE = OUInterop::ServerToUE(FVector(560000.f, 0.f, 0.f));
		const float MoonRadiusCm = 60000.f * OUInterop::ServerUnitToCm;
		// Близко к поверхности (15 км над ней) — рельеф в кадре.
		const FVector SpawnUE = MoonCenterUE + FVector(25000.f, 0.f, MoonRadiusCm + 15000.f);
		State.Position = OUInterop::UEToServer(SpawnUE);
		SetActorLocation(SpawnUE);
		// Тангаж вниз (в серверных осях Y — вверх, отрицательный — к поверхности).
		State.Pitch = -0.9f;
	}
	else if (FParse::Param(FCommandLine::Get(), TEXT("OUCombatDemo")))
	{
		// Диагностика: корабль рядом с боевыми целями (для снимка HUD боя).
		// Цели-дроны висят в 40/70/100 м впереди базы, ~15 м вверх.
		const FVector Base = AOUVoxelPlanet::HomeSpawn(2.0f);
		const FVector Up = Base.GetSafeNormal();
		FVector Fwd = FVector::VectorPlaneProject(FVector(1.f, 0.f, 0.f), Up).GetSafeNormal();
		if (Fwd.IsNearlyZero())
		{
			Fwd = FVector::VectorPlaneProject(FVector::ForwardVector, Up).GetSafeNormal();
		}
		const FVector SpawnUE = Base + Fwd * 3000.f;   // ~30 м впереди базы
		State.Position = OUInterop::UEToServer(SpawnUE);
		SetActorLocation(SpawnUE);
	}
	else if (GetActorLocation().IsNearlyZero())
	{
		const FVector SpawnUE = AOUVoxelPlanet::HomeSpawn(2.0f);
		State.Position = OUInterop::UEToServer(SpawnUE);
		SetActorLocation(SpawnUE);
	}
	else
	{
		State.Position = OUInterop::UEToServer(GetActorLocation());
	}

	// Находим клиент сети: свой компонент или на уровне.
	if (!Net)
	{
		Net = FindComponentByClass<UOUClientNet>();
		if (!Net)
		{
			Net = NewObject<UOUClientNet>(this, TEXT("OUClientNet"));
			Net->RegisterComponent();
		}
	}

	// Меш-заглушка для чужих кораблей (грузим один раз).
	if (!RemoteShipMesh)
	{
		RemoteShipMesh = LoadObject<UStaticMesh>(nullptr, TEXT("/Engine/BasicShapes/Cube.Cube"));
	}
	// Меш чужого персонажа (вертикальный цилиндр).
	if (!CharacterMesh)
	{
		CharacterMesh = LoadObject<UStaticMesh>(nullptr, TEXT("/Engine/BasicShapes/Cylinder.Cylinder"));
	}

	// Цвет корабля (оранжево-серый металл) — чтобы он читался в кадре.
	if (ShipMesh)
	{
		ShipMesh->SetVectorParameterValueOnMaterials(TEXT("Color"), FVector(0.85f, 0.55f, 0.20f));
	}

	// Менеджер начала координат (может отсутствовать — тогда работаем без него).
	OriginManager = Cast<AUOOriginManager>(
		UGameplayStatics::GetActorOfClass(GetWorld(), AUOOriginManager::StaticClass()));
}

void AOUShipPawn::UpdateRemoteCharacters()
{
	if (!Net)
	{
		return;
	}
	UWorld* World = GetWorld();
	if (!World)
	{
		return;
	}
	// Позы приходят каждый кадр — отмечаем видимых и рисуем «фигурки».
	for (const FOUCharacterState& C : Net->RemoteCharacters)
	{
		AActor* Actor = nullptr;
		if (TObjectPtr<AActor>* Found = RemoteCharacterActors.Find(C.Entity))
		{
			Actor = *Found;
		}
		if (!Actor)
		{
			Actor = World->SpawnActor<AActor>(AActor::StaticClass(), FTransform::Identity);
			if (!Actor) { continue; }
			USceneComponent* Root = NewObject<USceneComponent>(Actor, TEXT("Root"));
			Actor->SetRootComponent(Root);
			Root->RegisterComponent();
			// Простая фигурка: вертикальный цилиндр (для пеших игроков).
			UStaticMeshComponent* Comp = NewObject<UStaticMeshComponent>(Actor, TEXT("CharMesh"));
			Comp->SetupAttachment(Root);
			Comp->RegisterComponent();
			if (CharacterMesh)
			{
				Comp->SetStaticMesh(CharacterMesh);
			}
			Comp->SetRelativeScale3D(FVector(0.7f, 0.7f, 1.7f));
			Comp->SetCollisionEnabled(ECollisionEnabled::NoCollision);
			Comp->SetVectorParameterValueOnMaterials(TEXT("Color"), FVector(0.35f, 0.85f, 0.45f));
			Actor->AddInstanceComponent(Comp);
			RemoteCharacterActors.Add(C.Entity, Actor);
		}
		// C.Position — серверные оси: переводим в UE (с масштабом).
		const FVector Loc = OUInterop::ServerToUE(C.Position);
		Actor->SetActorLocation(Loc);
		if (!LoggedChars.Contains(C.Entity))
		{
			LoggedChars.Add(C.Entity);
			UE_LOG(LogOUShip, Log, TEXT("Сеть: видим пешего игрока %d"), C.Entity);
		}
	}
	// Очистка: если персонаж пропал (вернулся в корабль) — убираем.
	TSet<int32> Alive;
	for (const FOUCharacterState& C : Net->RemoteCharacters)
	{
		Alive.Add(C.Entity);
	}
	for (auto It = RemoteCharacterActors.CreateIterator(); It; ++It)
	{
		if (!Alive.Contains(It.Key()) && It.Value())
		{
			It.Value()->Destroy();
			It.RemoveCurrent();
		}
	}
}

void AOUShipPawn::DrawCombatEffects()
{
	if (!Net)
	{
		return;
	}

	// Смерть/возрождение своей сущности: прячем корабль, пока ждём возрождения.
	const bool bDead = Net->bOwnDestroyed;
	if (bDead != bWasDestroyed)
	{
		bWasDestroyed = bDead;
		if (ShipMesh)
		{
			ShipMesh->SetVisibility(!bDead);
		}
		UE_LOG(LogOUShip, Log, TEXT("Бой: своя сущность %s"),
			bDead ? TEXT("уничтожена, ждём возрождения") : TEXT("возрождена"));
	}

	if (Net->PendingShots.Num() == 0)
	{
		return;
	}
	UWorld* World = GetWorld();
	if (!World)
	{
		Net->PendingShots.Reset();
		return;
	}
	for (const FOUShotEvent& S : Net->PendingShots)
	{
		// Стрелок — либо своя сущность (корабль), либо чужой корабль рядом.
		FVector From = GetActorLocation();
		if (S.Attacker != State.Id)
		{
			if (TObjectPtr<AActor>* A = RemoteActors.Find(S.Attacker))
			{
				if (*A) { From = (*A)->GetActorLocation(); } else { continue; }
			}
			else { continue; }
		}
		UE_LOG(LogOUShip, Log, TEXT("Бой: выстрел %d → %d, попадание: %s, урон %d%s"),
			S.Attacker, S.Target, S.bHit ? TEXT("да") : TEXT("нет"), S.Damage,
			S.bDestroyed ? TEXT(", цель уничтожена") : TEXT(""));
		// Цель: чужая сущность или своя.
		FVector To = From + GetActorForwardVector() * 1500.f;   // промах — вперёд
		if (S.Target != 0)
		{
			if (S.Target == State.Id)
			{
				To = GetActorLocation();
			}
			else if (TObjectPtr<AActor>* TA = RemoteActors.Find(S.Target))
			{
				if (*TA) { To = (*TA)->GetActorLocation(); }
			}
		}
		const FColor Col = S.bHit ? FColor(255, 220, 80) : FColor(120, 160, 255);
		DrawDebugLine(World, From, To, Col, false, 0.15f, 0, 4.f);
		if (S.bHit)
		{
			DrawDebugPoint(World, To, 18.f, FColor(255, 120, 60), false, 0.15f);
		}
		// Цель уничтожена — убираем её актор с экрана сразу (в дельта-снимке
		// исчезновение сущности не приходит, только в baseline).
		if (S.bDestroyed && S.Target != 0)
		{
			if (TObjectPtr<AActor>* TA = RemoteActors.Find(S.Target))
			{
				if (*TA) { (*TA)->Destroy(); }
				RemoteActors.Remove(S.Target);
			}
		}
	}
	Net->PendingShots.Reset();
}

void AOUShipPawn::SetupPlayerInputComponent(UInputComponent* PlayerInputComponent)
{
	Super::SetupPlayerInputComponent(PlayerInputComponent);
	if (!PlayerInputComponent)
	{
		return;
	}
	// Классический ввод (без ассетов): работает сразу, без Blueprint.
	// Координатные оси не могут переиспользовать имена персонажа (‘Move’),
	// поэтому у корабля своя ось тяги и стрейфа; обзор — мышь.
	PlayerInputComponent->BindAxis(TEXT("Throttle"), this, &AOUShipPawn::AxisThrottle);
	PlayerInputComponent->BindAxis(TEXT("YawMouse"), this, &AOUShipPawn::AxisYawMouse);
	PlayerInputComponent->BindAxis(TEXT("PitchMouse"), this, &AOUShipPawn::AxisPitchMouse);
	PlayerInputComponent->BindAxis(TEXT("ShipStrafe"), this, &AOUShipPawn::AxisStrafe);
	PlayerInputComponent->BindAction(TEXT("Ascend"), IE_Pressed, this, &AOUShipPawn::OnAscendPressed);
	PlayerInputComponent->BindAction(TEXT("Ascend"), IE_Released, this, &AOUShipPawn::OnAscendReleased);
	PlayerInputComponent->BindAction(TEXT("Descend"), IE_Pressed, this, &AOUShipPawn::OnDescendPressed);
	PlayerInputComponent->BindAction(TEXT("Descend"), IE_Released, this, &AOUShipPawn::OnDescendReleased);
	// Огонь — ЛЕВАЯ кнопка мыши (в духе DU). Пробел теперь подъём, не огонь.
	PlayerInputComponent->BindAction(TEXT("ShipFire"), IE_Pressed, this, &AOUShipPawn::StartFire);
	PlayerInputComponent->BindAction(TEXT("ShipFire"), IE_Released, this, &AOUShipPawn::StopFire);
	PlayerInputComponent->BindAction(TEXT("ExitShip"), IE_Pressed, this, &AOUShipPawn::OnExitPressed);
	PlayerInputComponent->BindAction(TEXT("ExitShip"), IE_Released, this, &AOUShipPawn::OnExitReleased);
	PlayerInputComponent->BindAction(TEXT("CursorToggle"), IE_Pressed, this, &AOUShipPawn::OnCursorToggle);
	// Скрипты: L — окно скриптов объекта.
	PlayerInputComponent->BindAction(TEXT("ScriptMenu"), IE_Pressed, this, &AOUShipPawn::ToggleScripts);
	// Мастерская: K — обмен шаблонами.
	PlayerInputComponent->BindAction(TEXT("WorkshopMenu"), IE_Pressed, this, &AOUShipPawn::ToggleWorkshop);
	// Захват территории: T — объявить текущую зону своей (и в корабле).
	PlayerInputComponent->BindAction(TEXT("TerritoryCapture"), IE_Pressed, this, &AOUShipPawn::CaptureTerritory);
	// Сверхсвет: удержание Shift — сверхскоростной перелёт между телами системы.
	PlayerInputComponent->BindAction(TEXT("ShipBoost"), IE_Pressed, this, &AOUShipPawn::OnBoostPressed);
	PlayerInputComponent->BindAction(TEXT("ShipBoost"), IE_Released, this, &AOUShipPawn::OnBoostReleased);
}

void AOUShipPawn::AxisYawMouse(float Value)
{
	// Мышь: дельта за кадр — применяем сразу (без умножения на время кадра).
	// Копим и отправляем как обычный ввод, чтобы сервер видел тот же курс.
	if (bCursorCaptured)
	{
		MouseYawAccum += Value * MouseSensitivity;
	}
}

void AOUShipPawn::AxisPitchMouse(float Value)
{
	if (bCursorCaptured)
	{
		MousePitchAccum += Value * MouseSensitivity;
	}
}

void AOUShipPawn::OnCursorToggle()
{
	// Alt — отпустить/вернуть курсор (как в DU): пока курсор свободен, обзор
	// мышью и огонь не срабатывают, можно работать с интерфейсом.
	bCursorCaptured = !bCursorCaptured;
	if (APlayerController* PC = Cast<APlayerController>(GetController()))
	{
		PC->bShowMouseCursor = !bCursorCaptured;
		if (bCursorCaptured)
		{
			PC->SetInputMode(FInputModeGameOnly());
		}
		else
		{
			FInputModeGameAndUI Mode;
			Mode.SetHideCursorDuringCapture(false);
			PC->SetInputMode(Mode);
		}
	}
}

void AOUShipPawn::AxisStrafe(float Value)
{
	StrafeInput = FMath::Clamp(Value, -1.f, 1.f);
}

void AOUShipPawn::OnAscendPressed() { bAscend = true; }
void AOUShipPawn::OnAscendReleased() { bAscend = false; }
void AOUShipPawn::OnDescendPressed() { bDescend = true; }
void AOUShipPawn::OnDescendReleased() { bDescend = false; }
void AOUShipPawn::OnBoostPressed() { bBoost = true; }
void AOUShipPawn::OnBoostReleased() { bBoost = false; }

void AOUShipPawn::OnExitPressed()
{
	// Зажимаем F: копим время удержания, выход — по порогу (не мгновенно).
	bExitHeld = true;
	ExitHoldTime = 0.f;
}

void AOUShipPawn::OnExitReleased()
{
	bExitHeld = false;
	ExitHoldTime = 0.f;
}

void AOUShipPawn::CaptureTerritory()
{
	// Захват зоны, в которой находится корабль (клавиша T).
	if (!Net)
	{
		Net = FindComponentByClass<UOUClientNet>();
	}
	if (!Net || !Net->bTerritoriesReceived || Net->CurrentTerritoryId == 0)
	{
		return;
	}
	Net->SendCapture(Net->CurrentTerritoryId);
	UE_LOG(LogOUShip, Log, TEXT("Территория: запрос захвата зоны %d"), Net->CurrentTerritoryId);
}

void AOUShipPawn::ToggleScripts()
{
	// Окно скриптов объекта (клавиша L) — работает и из корабля.
	AOUPlayerMenuController* Menu = Cast<AOUPlayerMenuController>(GetController());
	if (!Menu)
	{
		Menu = Cast<AOUPlayerMenuController>(GetWorld() ? GetWorld()->GetFirstPlayerController() : nullptr);
	}
	if (Menu)
	{
		Menu->bScriptsOpen = !Menu->bScriptsOpen;
		if (Menu->bScriptsOpen)
		{
			Menu->bMarketOpen = false;
			Menu->bWorkshopOpen = false;
		}
		Menu->SetMenuCursor(Menu->bScriptsOpen);
		UE_LOG(LogOUShip, Log, TEXT("Скрипты: %s"), Menu->bScriptsOpen ? TEXT("открыто") : TEXT("закрыто"));
	}
}

void AOUShipPawn::ToggleWorkshop()
{
	// Окно мастерской (клавиша K) — работает и из корабля.
	AOUPlayerMenuController* Menu = Cast<AOUPlayerMenuController>(GetController());
	if (!Menu)
	{
		Menu = Cast<AOUPlayerMenuController>(GetWorld() ? GetWorld()->GetFirstPlayerController() : nullptr);
	}
	if (Menu)
	{
		Menu->bWorkshopOpen = !Menu->bWorkshopOpen;
		if (Menu->bWorkshopOpen)
		{
			Menu->bMarketOpen = false;
			Menu->bScriptsOpen = false;
		}
		Menu->SetMenuCursor(Menu->bWorkshopOpen);
		UE_LOG(LogOUShip, Log, TEXT("Мастерская: %s"), Menu->bWorkshopOpen ? TEXT("открыто") : TEXT("закрыто"));
	}
}

void AOUShipPawn::ExitShip()
{
	UWorld* World = GetWorld();
	if (!World)
	{
		return;
	}
	// Точка рядом с кораблём, но подальше от построек базы, чтобы персонаж
	// оказался на открытой поверхности.
	const FVector ShipPos = GetActorLocation();
	const FVector Up = ShipPos.GetSafeNormal();
	FVector Side = FVector::CrossProduct(Up, GetActorForwardVector()).GetSafeNormal();
	if (Side.IsNearlyZero())
	{
		Side = FVector::CrossProduct(Up, FVector::RightVector).GetSafeNormal();
	}
	FVector Spawn = ShipPos + Side * 4000.f;
	// Привязываем к рельефу.
	const FVector Dir = Spawn.GetSafeNormal();
	if (AOUVoxelPlanet* Planet = AOUVoxelPlanet::FindHomePlanet(World))
	{
		Spawn = Dir * (Planet->SurfaceRadiusAt(Dir) + 95.f);
	}

	if (!Character)
	{
		Character = World->SpawnActor<AOUCharacterPawn>(
			AOUCharacterPawn::StaticClass(), FTransform(FRotator::ZeroRotator, Spawn));
	}
	if (Character)
	{
		Character->Ship = this;
		// Курс — от базы наружу (в сторону, куда отошли).
		const float LookYaw = FMath::Atan2(Side.Y, Side.X);
		Character->PlaceAt(Spawn, LookYaw);
		// Передаём управление персонажу.
		if (AController* C = GetController())
		{
			APlayerController* PC = Cast<APlayerController>(C);
			if (PC)
			{
				PC->Possess(Character);
			}
		}
		UE_LOG(LogOUShip, Log, TEXT("Выход из корабля: персонаж создан"));
	}
}

void AOUShipPawn::EnterShip(AOUCharacterPawn* FromCharacter)
{
	// Возврат в кабину: управление снова у корабля.
	if (AController* C = Cast<AController>(FromCharacter ? FromCharacter->GetController() : GetController()))
	{
		APlayerController* PC = Cast<APlayerController>(C);
		if (PC)
		{
			PC->Possess(this);
		}
	}
	if (FromCharacter)
	{
		FromCharacter->Destroy();
		Character = nullptr;
	}
	// Сообщаем серверу, что пеший персонаж больше не активен (снять позу).
	if (Net && Net->IsConnected())
	{
		Net->SendCharacter(FVector::ZeroVector, 0.f, false, /*bActive=*/false);
	}
	UE_LOG(LogOUShip, Log, TEXT("Возврат в корабль"));
}

void AOUShipPawn::AxisThrottle(float Value)
{
	CurrentInput.Throttle = FMath::Clamp(Value, -1.f, 1.f);
}

void AOUShipPawn::AxisYaw(float Value)
{
	CurrentInput.Yaw = FMath::Clamp(Value, -1.f, 1.f);
}

void AOUShipPawn::AxisPitch(float Value)
{
	CurrentInput.Pitch = FMath::Clamp(Value, -1.f, 1.f);
}

void AOUShipPawn::StartFire()
{
	CurrentInput.bFire = true;
}

void AOUShipPawn::StopFire()
{
	CurrentInput.bFire = false;
}

void AOUShipPawn::StepPrediction(const FOUInputFrame& Input, float Dt)
{
	using namespace OUDeterministic;

	// 1. Углы (как на сервере: yaw и pitch хранятся в состоянии).
	double Yaw = State.Yaw + Input.Yaw * Flight.MaxYawRate * Dt;
	double Pitch = State.Pitch + Input.Pitch * Flight.MaxPitchRate * Dt;
	Yaw = WrapPi(Yaw);
	Pitch = WrapPi(Pitch);
	const double PitchLimit = OUPi / 2.0 - 0.02;
	Pitch = FMath::Clamp(Pitch, -PitchLimit, PitchLimit);
	State.Yaw = static_cast<float>(Yaw);
	State.Pitch = static_cast<float>(Pitch);

	// 2. Направление «вперёд» из углов (та же формула, что на сервере).
	const FVector Forward(
		DetCos(Pitch) * DetSin(Yaw),
		DetSin(Pitch),
		DetCos(Pitch) * DetCos(Yaw));

	// Локальные оси: «вверх» — мировой +Y, «вправо» — up × forward, «локальный
	// верх» — forward × right. Как на сервере (проверено тестом).
	const FVector WorldUp(0.0, 1.0, 0.0);
	FVector Right = FVector::CrossProduct(WorldUp, Forward);
	Right = Right.SizeSquared() > 1e-12 ? Right.GetSafeNormal() : FVector::ZeroVector;
	FVector UpLocal = FVector::CrossProduct(Forward, Right);
	UpLocal = UpLocal.SizeSquared() > 1e-12 ? UpLocal.GetSafeNormal() : WorldUp;

	// 3. Тяга + стрейф + подъём/спуск + демпфирование + гравитация планеты (как
	//    на сервере: `integrate_with_accel` добавляет внешнее ускорение до шага).
	// Сверхсвет (boost) — те же множители, что на сервере, иначе предсказание
	// разойдётся с авторитетным состоянием.
	const double BoostThrustMul = Input.bBoost ? 400.0 : 1.0;
	const double BoostSpeedMul = Input.bBoost ? 400.0 : 1.0;
	const FVector Thrust = Forward * (Input.Throttle * Flight.MaxThrust * BoostThrustMul);
	const FVector Side = Right * (Input.Strafe * Flight.MaxStrafeThrust);
	const FVector Lift = UpLocal * (Input.Up * Flight.MaxStrafeThrust);
	const FVector Damp = State.Velocity * (-Flight.LinearDamping);
	// Гравитация от ВСЕХ тел системы (домашняя планета + соседние сферы) —
	// та же сумма, что на сервере, иначе предсказание разойдётся.
	const FVector Gravity = GravityAtAll(State.Position);
	State.Velocity += (Thrust + Side + Lift + Damp + Gravity) * Dt;

	// 4. Ограничение скорости (в сверхсвете лимит выше).
	const double Speed = State.Velocity.Size();
	const double SpeedCap = Flight.MaxSpeed * BoostSpeedMul;
	if (Speed > SpeedCap)
	{
		State.Velocity = State.Velocity * (SpeedCap / Speed);
	}

	// 5. Позиция.
	State.Position += State.Velocity * Dt;

	// 6. Столкновение с поверхностью планеты — как `resolve_ground` на сервере.
	// Без этого гравитация проваливает корабль сквозь планету, и он «не на
	// планете». Корабль не может быть ниже поверхности.
	//
	// ВАЖНО: State.Position — в СЕРВЕРНЫХ единицах (радиус планеты = 100000),
	// поэтому и поверхность считаем в тех же единицах: у актора Radius и
	// Amplitude в сантиметрах UE, приводим их делением на масштаб.
	const FVector Rel = State.Position;                    // серверные оси
	const double Dist = Rel.Size();
	if (Dist > 1e-6)
	{
		const FVector UpS = Rel / Dist;                    // серверные оси
		AOUVoxelPlanet* Planet = AOUVoxelPlanet::FindHomePlanet(GetWorld());
		// Поверхность в серверных единицах (Radius/Amplitude актора → у.е.).
		const double ToWorld = 1.0 / OUInterop::ServerUnitToCm;
		const double SurfaceR = Planet
			? Planet->SurfaceRadiusAtServer(UpS) * ToWorld
			: 500000.0;
		if (Dist < SurfaceR)
		{
			// Прижимаем к поверхности и гасим радиальную (внутрь) скорость.
			State.Position = UpS * SurfaceR;
			const double VNorm = State.Velocity.Dot(UpS);
			if (VNorm < 0.0)
			{
				State.Velocity -= UpS * VNorm;
			}
		}
		// Соседние тела: гладкие сферы — прижать, если под поверхностью.
		if (const UOUClientNet* NetC = FindComponentByClass<UOUClientNet>())
		{
			for (const FOUSystemBody& B : NetC->SystemBodies)
			{
				if (B.Radius <= 0.f || (B.Center.IsNearlyZero() && B.Radius > 400000.f))
				{
					continue;
				}
				const FVector RelB = State.Position - B.Center;
				const double DB = RelB.Size();
				if (DB < 1.0 || DB >= B.Radius)
				{
					continue;
				}
				const FVector UpB = RelB / DB;
				// Поверхность тела с рельефом: радиус + высота общей формулы.
				// Оси тела на клиенте — серверные (как у планетного меша).
				double SurfaceR_B = B.Radius;
				if (B.Amplitude > 0.0)
				{
					const double HB = OULT::HeightDir(
						UpB.X, UpB.Y, UpB.Z, B.Amplitude, 0.545, static_cast<uint64_t>(B.Seed), 1);
					SurfaceR_B = B.Radius + HB;
				}
				if (DB >= SurfaceR_B)
				{
					continue;
				}
				State.Position = B.Center + UpB * SurfaceR_B;
				const double VN = State.Velocity.Dot(UpB);
				if (VN < 0.0)
				{
					State.Velocity -= UpB * VN;
				}
			}
		}
	}

	ApplyStateToActor();
}

AOUPlayerMenuController* AOUShipPawn::GetMenuController() const
{
	// Контроллер игрока может быть меню; если да — отдаём его.
	return Cast<AOUPlayerMenuController>(GetController());
}

void AOUShipPawn::ApplyStateToActor()
{
	// Позиция: State.Position (серверные оси) → UE.
	const FVector PosUE = OUInterop::ServerToUE(State.Position);
	if (OriginManager)
	{
		OriginManager->SetWorldPosition(this, PosUE);
	}
	else
	{
		SetActorLocation(PosUE);
	}

	// Ориентация. На поверхности планеты «верх» — это радиус от центра, а не
	// глобальный +Z. Поэтому строим поворот так, чтобы локальный «верх» смотрел
	// от центра планеты, а «вперёд» — по касательной. Иначе камера смотрит
	// в небо и ландшафта не видно.
	const FVector FwdUE = OUInterop::ServerDirToUE(ForwardVector()).GetSafeNormal();
	const FVector UpRadial = PosUE.GetSafeNormal();          // от центра планеты
	const FVector NearSurface = UpRadial.SizeSquared() > 0.5f ? UpRadial : FVector::UpVector;

	// Вычитаем радиальную составляющую из «вперёд»: остаётся касательная.
	FVector Tangent = FwdUE - NearSurface * FVector::DotProduct(FwdUE, NearSurface);
	if (Tangent.SizeSquared() < 0.01f)
	{
		// Вперёд почти совпал с радиусом — берём любую касательную.
		Tangent = FVector::CrossProduct(NearSurface, FVector::RightVector);
	}
	Tangent = Tangent.GetSafeNormal();

	SetActorRotation(FRotationMatrix::MakeFromXZ(Tangent, NearSurface).Rotator());
}

FVector AOUShipPawn::ForwardVector() const
{
	using namespace OUDeterministic;
	// Вперёд в серверных осях (вверх = Y): (cp·sy, sp, cp·cy).
	const double CP = DetCos(State.Pitch);
	const double SP = DetSin(State.Pitch);
	const double SY = DetSin(State.Yaw);
	const double CY = DetCos(State.Yaw);
	return FVector(CP * SY, SP, CP * CY);
}

FVector AOUShipPawn::ForwardVectorOf(const FOUEntityState& E)
{
	using namespace OUDeterministic;
	const double CP = DetCos(E.Pitch);
	const double SP = DetSin(E.Pitch);
	const double SY = DetSin(E.Yaw);
	const double CY = DetCos(E.Yaw);
	return FVector(CP * SY, SP, CP * CY);
}

void AOUShipPawn::ApplyAuthoritative(const FOUEntityState& Authoritative)
{
	// Реконсиляция: встать на авторитетную точку. Полноценная версия
	// переигрывает неподтверждённые кадры (см. `ou_client::reconcile`).
	State.Position = Authoritative.Position;
	State.Velocity = Authoritative.Velocity;
	State.Yaw = Authoritative.Yaw;
	State.Pitch = Authoritative.Pitch;

	ApplyStateToActor();
}

void AOUShipPawn::Tick(float DeltaSeconds)
{
	Super::Tick(DeltaSeconds);

	// Пока открыто стартовое меню (персонаж/сервер) — корабль стоит на месте и
	// ввод не отправляется: игрок сначала выбирает персонажа и сервер.
	if (AOUPlayerMenuController* Menu = GetMenuController())
	{
		if (Menu->IsMenuOpen())
		{
			ApplyStateToActor();
			return;
		}
	}

	// Корабль без пилота (игрок ходит пешком): он «припаркован» там, где его
	// поставил персонаж. Сеть продолжаем опрашивать (нужно для подключения и
	// своего id), но НЕ реконсилимся с сервером: иначе корабль «уезжал» бы к
	// серверной точке появления и пропадал из кадра.
	if (GetController() == nullptr)
	{
		if (Net)
		{
			Net->Poll();
			if (Net->OwnEntityId != 0)
			{
				State.Id = Net->OwnEntityId;
			}
			if (Net->IsConnected())
			{
				// Рисуем других (дроны/чужие корабли и персонажи).
				UpdateRemotePlayers();
				UpdateRemoteCharacters();
				// Тела системы больше не строятся: в мире одна планета,
				// клиент создаёт её сам из общих констант.
				// Тестовый авто-огонь работает и у припаркованного корабля:
				// шлём кадр с огнём (без тяги), чтобы проверить бой.
				if (FParse::Param(FCommandLine::Get(), TEXT("OUAutoFire")))
				{
					FireTimer += DeltaSeconds;
					if (FireTimer >= 0.5f)
					{
						FireTimer = 0.f;
						FOUInputFrame Fire;
						Fire.Seq = ++InputSeqAuto;
						Fire.bFire = true;
						Net->SendInput(Fire);
					}
				}
			}
		}
		// Трассеры/попадания по событиям от сервера.
		DrawCombatEffects();
		ApplyStateToActor();
		return;
	}

	// Выход из корабля по **зажатию** F: копим время, по порогу — выходим.
	if (bExitHeld)
	{
		ExitHoldTime += DeltaSeconds;
		if (ExitHoldTime >= ExitHoldThreshold)
		{
			ExitHoldTime = 0.f;
			bExitHeld = false;
			ExitShip();
			return;   // управление ушло персонажу
		}
	}

	// Фиксированный шаг: сеть и предсказание идут на 30 Гц, рендер — быстрее.
	const int32 Steps = FMath::Max(1, FMath::RoundToInt(DeltaSeconds / FixedDeltaSeconds));

	// Тестовый авто-огонь (ключ `-OUAutoFire`): стреляем, чтобы проверить бой.
	if (FParse::Param(FCommandLine::Get(), TEXT("OUAutoFire")))
	{
		FireTimer += DeltaSeconds;
		CurrentInput.bFire = FireTimer >= 0.5f;
		if (CurrentInput.bFire)
		{
			FireTimer = 0.f;
		}
	}

	// Мышь: превращаем накопленный за кадр курс/тангаж в управляющий ввод.
	// Знак и величина те же, что у клавиатуры (Yaw/Pitch), поэтому сервер видит
	// привычный кадр и предсказание совпадает.
	if (MouseYawAccum != 0.f || MousePitchAccum != 0.f)
	{
		CurrentInput.Yaw = FMath::Clamp(CurrentInput.Yaw + MouseYawAccum * 40.f, -1.f, 1.f);
		CurrentInput.Pitch = FMath::Clamp(CurrentInput.Pitch + MousePitchAccum * 40.f, -1.f, 1.f);
		MouseYawAccum = 0.f;
		MousePitchAccum = 0.f;
	}
	// Стрейф (A/D) и подъём/спуск (Пробел/Ctrl) — как в Dual Universe.
	CurrentInput.Strafe = StrafeInput;
	CurrentInput.Up = (bAscend ? 1.f : 0.f) + (bDescend ? -1.f : 0.f);
	// Сверхсвет: перелёт между телами системы.
	CurrentInput.bBoost = bBoost;

	// Предсказываем движение нашим же шагом (сеть не требуется).
	CurrentInput.Seq += 1;
	for (int32 i = 0; i < Steps; ++i)
	{
		StepPrediction(CurrentInput, FixedDeltaSeconds);
	}

	if (Net)
	{
		// Опрашиваем сеть всегда: так мы узнаём свой id и состояние мира.
		Net->Poll();

		// Свой id берём у сервера (он сообщает его в welcome).
		if (Net->OwnEntityId != 0)
		{
			State.Id = Net->OwnEntityId;
		}

		if (Net->IsConnected())
		{
			if (bInputEnabled)
			{
				Net->SendInput(CurrentInput);
				// Запоминаем кадр: сервер подтвердит его seq, и мы отбросим.
				PendingInputs.Add(CurrentInput);
				if (PendingInputs.Num() > 128)
				{
					PendingInputs.RemoveAt(0);
				}
			}
			// Какая территория под кораблём (для HUD и клавиши захвата T).
			Net->UpdateTerritoryAt(State.Position.X, State.Position.Y, State.Position.Z);
			// Диагностика `-OUMoonOrbit`/`-OUCombatDemo`: не реконсилимся —
			// держим корабль на месте для снимка.
			if (!FParse::Param(FCommandLine::Get(), TEXT("OUMoonOrbit")) &&
				!FParse::Param(FCommandLine::Get(), TEXT("OUCombatDemo")))
			{
				// Реконсиляция: принимаем авторитетное состояние своей сущности
				// и переигрываем ещё не подтверждённые кадры.
				for (const FOUEntityState& E : Net->LatestSnapshot.Entities)
				{
					if (E.Id == State.Id)
					{
						ReconcileTo(E, Net->LatestSnapshot.AckInputSeq);
						break;
					}
				}
			}
			// Рисуем чужие корабли из последнего снимка.
			UpdateRemotePlayers();
			UpdateRemoteCharacters();
		}
		// Трассеры и попадания по событиям выстрела от сервера.
		DrawCombatEffects();
	}

	// Переносим начало координат, если улетели далеко.
	if (OriginManager)
	{
		OriginManager->UpdateOrigin(this);
	}
}

void AOUShipPawn::ReconcileTo(const FOUEntityState& Authoritative, int32 AckSeq)
{
	// 1. Встаём на авторитетное состояние.
	ApplyAuthoritative(Authoritative);
	// 2. Отбрасываем подтверждённые кадры.
	PendingInputs.RemoveAll([AckSeq](const FOUInputFrame& In) { return In.Seq <= AckSeq; });
	// 3. Переигрываем оставшиеся — так предсказание снова совпадёт с сервером.
	for (const FOUInputFrame& In : PendingInputs)
	{
		StepPrediction(In, FixedDeltaSeconds);
	}
}

AActor* AOUShipPawn::GetOrCreateRemoteActor(int32 EntityId)
{
	if (TObjectPtr<AActor>* Found = RemoteActors.Find(EntityId))
	{
		return *Found;
	}
	UWorld* World = GetWorld();
	if (!World)
	{
		return nullptr;
	}
	AActor* Actor = World->SpawnActor<AActor>(AActor::StaticClass(), FTransform::Identity);
	if (!Actor)
	{
		return nullptr;
	}
	UStaticMeshComponent* Comp = NewObject<UStaticMeshComponent>(Actor);
	Comp->SetupAttachment(Actor->GetRootComponent());
	Comp->RegisterComponent();
	if (RemoteShipMesh)
	{
		Comp->SetStaticMesh(RemoteShipMesh);
	}
	Comp->SetRelativeScale3D(FVector(3.f, 1.f, 0.6f));
	Comp->SetCollisionEnabled(ECollisionEnabled::NoCollision);
	Actor->AddInstanceComponent(Comp);
	// Красноватый оттенок, чтобы отличать чужие корабли от своего.
	Comp->SetVectorParameterValueOnMaterials(TEXT("Color"), FVector(1.f, 0.35f, 0.35f));
	RemoteActors.Add(EntityId, Actor);
	return Actor;
}

void AOUShipPawn::UpdateRemotePlayers()
{
	const FOUSnapshot& Snap = Net->LatestSnapshot;
	TSet<int32> Seen;
	for (const FOUEntityState& E : Snap.Entities)
	{
		if (E.Id == State.Id)
		{
			continue; // свой корабль уже есть
		}
		Seen.Add(E.Id);
		AActor* Actor = GetOrCreateRemoteActor(E.Id);
		if (Actor)
		{
			// E.Position — в серверных осях: переводим в UE (с масштабом).
			Actor->SetActorLocation(OUInterop::ServerToUE(E.Position));
			// Чужие корабли смотрят туда же, куда летят.
			const FVector FwdUE = OUInterop::ServerDirToUE(ForwardVectorOf(E));
			Actor->SetActorRotation(FwdUE.Rotation());
		}
	}

	// Снимки — дельта: отсутствие сущности в дельте НЕ значит, что она ушла.
	// Полный список видимых сущностей приходит только в baseline, поэтому
	// удаляем «призраков» лишь тогда, когда это действительно достоверно.
	if (Snap.bBaseline)
	{
		for (auto It = RemoteActors.CreateIterator(); It; ++It)
		{
			if (!Seen.Contains(It.Key()) && It.Value())
			{
				It.Value()->Destroy();
				It.RemoveCurrent();
			}
		}
	}
}
