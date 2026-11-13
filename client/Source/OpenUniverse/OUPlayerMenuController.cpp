// Copyright Open Universe. Все права защищены.

#include "OUPlayerMenuController.h"
#include "OUClientNet.h"
#include "OUShipPawn.h"
#include "OUCharacterPawn.h"
#include "OpenUniverseTypes.h"
#include "Engine/Canvas.h"
#include "Engine/Engine.h"
#include "Engine/Font.h"
#include "EngineUtils.h"
#include "GameFramework/Pawn.h"
#include "GameFramework/PlayerInput.h"
#include "HttpModule.h"
#include "Interfaces/IHttpRequest.h"
#include "Interfaces/IHttpResponse.h"
#include "Dom/JsonObject.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Kismet/GameplayStatics.h"

DEFINE_LOG_CATEGORY_STATIC(LogOUMenu, Log, All);

const TCHAR* AOUPlayerMenuController::HealthUrl =
	TEXT("https://andreyshpak2016-lgtm.github.io/open-universe/status.json");

namespace
{
	// Тёмная палитра сайта/лаунчера, чтобы меню выглядело единообразно.
	const FLinearColor ColBg(0.02f, 0.03f, 0.06f, 0.86f);
	const FLinearColor ColPanel(0.07f, 0.09f, 0.19f, 0.96f);
	const FLinearColor ColPanelBorder(0.30f, 0.38f, 0.75f, 1.f);
	const FLinearColor ColAccent(0.42f, 0.55f, 1.0f, 1.f);
	const FLinearColor ColAccent2(0.0f, 0.90f, 0.82f, 1.f);
	const FLinearColor ColText(0.90f, 0.93f, 1.0f, 1.f);
	const FLinearColor ColMuted(0.58f, 0.63f, 0.77f, 1.f);
	const FLinearColor ColOk(0.35f, 0.86f, 0.63f, 1.f);
	const FLinearColor ColErr(1.0f, 0.45f, 0.48f, 1.f);
	const FLinearColor WarmDot(1.0f, 0.72f, 0.30f, 1.f);

	/** Заливка прямоугольника (не путать с DrawBox из SceneManagement). */
	void FillBox(UCanvas* InCanvas, const FVector2D& Pos, const FVector2D& Size, const FLinearColor& Color)
	{
		FCanvasTileItem Tile(Pos, Size, Color);
		Tile.BlendMode = SE_BLEND_AlphaBlend;
		InCanvas->DrawItem(Tile);
	}

	void StrokeBox(UCanvas* InCanvas, const FVector2D& Pos, const FVector2D& Size, const FLinearColor& Color, float Thickness)
	{
		FillBox(InCanvas, Pos, FVector2D(Size.X, Thickness), Color);
		FillBox(InCanvas, Pos + FVector2D(0, Size.Y - Thickness), FVector2D(Size.X, Thickness), Color);
		FillBox(InCanvas, Pos, FVector2D(Thickness, Size.Y), Color);
		FillBox(InCanvas, Pos + FVector2D(Size.X - Thickness, 0), FVector2D(Thickness, Size.Y), Color);
	}

	void DrawText(UCanvas* InCanvas, UFont* Font, const FString& Text, const FVector2D& Pos, const FLinearColor& Color)
	{
		FCanvasTextItem Item(Pos, FText::FromString(Text), Font, Color);
		Item.EnableShadow(FLinearColor(0, 0, 0, 0.6f));
		InCanvas->DrawItem(Item);
	}

	/** Вертикальный градиент (сверху → снизу) — «стекло» панелей. */
	void FillGradientV(UCanvas* InCanvas, const FVector2D& Pos, const FVector2D& Size, const FLinearColor& Top, const FLinearColor& Bottom, int32 Steps = 12)
	{
		for (int32 i = 0; i < Steps; ++i)
		{
			const float T0 = static_cast<float>(i) / Steps;
			const float T1 = static_cast<float>(i + 1) / Steps;
			const FLinearColor C = FLinearColor::LerpUsingHSV(Top, Bottom, T0);
			FillBox(InCanvas, Pos + FVector2D(0, Size.Y * T0), FVector2D(Size.X, Size.Y * (T1 - T0) + 1.f), C);
		}
	}

	/** «Стеклянная» панель: градиент, рамка и акцентная верхняя кромка. */
	void GlassPanel(UCanvas* InCanvas, const FVector2D& Pos, const FVector2D& Size, const FLinearColor& Border, const FLinearColor& Accent)
	{
		FillGradientV(InCanvas, Pos, Size, FLinearColor(0.08f, 0.10f, 0.20f, 0.94f), FLinearColor(0.03f, 0.04f, 0.09f, 0.94f));
		StrokeBox(InCanvas, Pos, Size, Border, 1.5f);
		FillBox(InCanvas, Pos, FVector2D(Size.X, 3.f), Accent);            // верхняя акцентная кромка
		FillBox(InCanvas, Pos, FVector2D(4.f, Size.Y), Accent * 0.6f);    // левая кромка
	}

	/** Горизонтальная шкала со значением (для HUD). */
	void Bar(UCanvas* InCanvas, const FVector2D& Pos, const FVector2D& Size, float Fraction, const FLinearColor& Fill, const FLinearColor& Back)
	{
		FillBox(InCanvas, Pos, Size, Back);
		const float F = FMath::Clamp(Fraction, 0.f, 1.f);
		FillBox(InCanvas, Pos, FVector2D(Size.X * F, Size.Y), Fill);
		StrokeBox(InCanvas, Pos, Size, FLinearColor(0.5f, 0.6f, 0.9f, 0.5f), 1.f);
	}
}

AOUPlayerMenuController::AOUPlayerMenuController()
{
	// Меню рисуется из DrawHUD, поэтому HUD нужен.
	bShowMouseCursor = true;
	PrimaryActorTick.bCanEverTick = true;
}

void AOUPlayerMenuController::BeginPlay()
{
	Super::BeginPlay();
	bMenuOpen = true;
	Screen = EOUMenuScreen::Character;

	// Список серверов. Авто-подключение перебирает их по порядку и
	// подключается к первому, кто ответит. Список можно расширять.
	Servers.Reset();
	Servers.Add(FOUServerInfo(TEXT("Локальный (свой ПК)"), TEXT("127.0.0.1"), 7777));
	Servers.Add(FOUServerInfo(TEXT("Open Universe (публичный)"), TEXT("127.0.0.1"), 7777));

	// --- Авто-подключение для тестов/автоматики: -OUAutoConnect ---
	const TCHAR* CmdLine = FCommandLine::Get();
	bAutoConnect = FParse::Param(CmdLine, TEXT("OUAutoConnect"));
	bAutoExit = FParse::Param(CmdLine, TEXT("OUAutoExit"));
	FString NameParam;
	if (FParse::Value(CmdLine, TEXT("OUName="), NameParam) && !NameParam.IsEmpty())
	{
		AutoConnectName = NameParam;
	}
	if (bAutoConnect)
	{
		Profile.Name = AutoConnectName.IsEmpty() ? TEXT("pilot") : AutoConnectName;
		UE_LOG(LogOUMenu, Log, TEXT("Авто-подключение включено (пилот «%s»)"), *Profile.Name);
	}

	SetMenuCursor(true);
	FetchHealth();
	PingServers();
}

void AOUPlayerMenuController::SetupInputComponent()
{
	Super::SetupInputComponent();
	if (!InputComponent)
	{
		return;
	}
	InputComponent->BindAction("MenuClick", IE_Pressed, this, &AOUPlayerMenuController::OnLeftClick);
	InputComponent->BindAction("MenuBackspace", IE_Pressed, this, &AOUPlayerMenuController::OnBackspace);
}

void AOUPlayerMenuController::ResolveNet()
{
	if (Net)
	{
		return;
	}
	// Сетевой компонент ищем по всем пешкам: игрок появляется персонажем, но
	// подключение (и симуляция корабля) живут в корабле рядом.
	UWorld* World = GetWorld();
	if (!World)
	{
		return;
	}
	for (TActorIterator<APawn> It(World); It; ++It)
	{
		if (UOUClientNet* Found = It->FindComponentByClass<UOUClientNet>())
		{
			Net = Found;
			Ship = Cast<AOUShipPawn>(*It);
			return;
		}
	}
	// Если сети нигде нет — создаём её на корабле (или на своей пешке).
	AOUShipPawn* HomeShip = nullptr;
	for (TActorIterator<AOUShipPawn> It(World); It; ++It) { HomeShip = *It; break; }
	AActor* NetOwner = HomeShip ? static_cast<AActor*>(HomeShip) : static_cast<AActor*>(GetPawn());
	if (NetOwner)
	{
		Net = NewObject<UOUClientNet>(NetOwner, TEXT("OUClientNet"));
		Net->RegisterComponent();
		Ship = HomeShip;
	}
}

void AOUPlayerMenuController::SetConnectStatus(const FString& Text)
{
	ConnectStatus = Text;
	UE_LOG(LogOUMenu, Log, TEXT("Статус меню: %s"), *Text);
}

void AOUPlayerMenuController::PingServers()
{
	// Лёгкая проверка: пробуем создать сокет и отправить hello. Полноценную
	// проверку делает серверная утилита ou-health; здесь — быстрый отклик для UI.
	for (FOUServerInfo& S : Servers)
	{
		// Пока помечаем как «неизвестно»; статус сервер+клиент покажет FetchHealth.
		S.bOnline = false;
	}
}

void AOUPlayerMenuController::FetchHealth()
{
	// Читаем status.json с сайта: там свежий статус сервер+клиент.
	TSharedRef<IHttpRequest, ESPMode::ThreadSafe> Req = FHttpModule::Get().CreateRequest();
	Req->SetURL(HealthUrl);
	Req->SetVerb(TEXT("GET"));
	Req->OnProcessRequestComplete().BindLambda(
		[this](FHttpRequestPtr, FHttpResponsePtr Resp, bool bOk)
		{
			if (!bOk || !Resp.IsValid())
			{
				Health.bKnown = false;
				UE_LOG(LogOUMenu, Warning, TEXT("status.json недоступен"));
				return;
			}
			TSharedPtr<FJsonObject> Root;
			const TSharedRef<TJsonReader<>> Reader = TJsonReaderFactory<>::Create(Resp->GetContentAsString());
			if (!FJsonSerializer::Deserialize(Reader, Root) || !Root.IsValid())
			{
				Health.bKnown = false;
				return;
			}
			Health.bKnown = true;
			Root->TryGetBoolField(TEXT("ok"), Health.bOk);
			Root->TryGetBoolField(TEXT("server"), Health.bServer);
			Root->TryGetBoolField(TEXT("client"), Health.bClient);
			Root->TryGetBoolField(TEXT("simulation"), Health.bSimulation);
			Root->TryGetBoolField(TEXT("spawnOnPlanet"), Health.bSpawnOnPlanet);
			int32 Tick = 0;
			Root->TryGetNumberField(TEXT("tick"), Tick);
			Health.Tick = Tick;
			double Lat = 0.0;
			Root->TryGetNumberField(TEXT("latencyMs"), Lat);
			Health.LatencyMs = static_cast<float>(Lat);
			Root->TryGetStringField(TEXT("notes"), Health.Notes);
			UE_LOG(LogOUMenu, Log, TEXT("status.json: ok=%d tick=%d"), Health.bOk ? 1 : 0, Health.Tick);
		});
	Req->ProcessRequest();
}

void AOUPlayerMenuController::Tick(float DeltaSeconds)
{
	Super::Tick(DeltaSeconds);

	// Когда Pawn появился, находим сеть и следим за подключением.
	if (Screen == EOUMenuScreen::InGame)
	{
		ResolveNet();
	}

	// --- Авто-подключение (для тестов и автоматики) ---
	if (bAutoConnect && bMenuOpen)
	{
		if (!bAutoConnectStarted)
		{
			bAutoConnectStarted = true;
			StartAutoConnect();
		}
		AutoConnectStep();
	}

	// Диагностика: `-OUPauseMenu` — открыть пауза-меню (ESC) через пару секунд
	// после входа в игру, чтобы снять кадр без ручных нажатий.
	if (FParse::Param(FCommandLine::Get(), TEXT("OUPauseMenu")) && !bPauseOpen && !bMenuOpen)
	{
		PauseShotTimer += DeltaSeconds;
		if (PauseShotTimer >= 3.f)
		{
			OpenPauseMenu();
			if (FParse::Param(FCommandLine::Get(), TEXT("OUSettings")))
			{
				Screen = EOUMenuScreen::Settings;
			}
		}
	}
}

void AOUPlayerMenuController::SetMenuCursor(bool bShow)
{
	bShowMouseCursor = bShow;
	if (bShow)
	{
		FInputModeGameAndUI Mode;
		Mode.SetLockMouseToViewportBehavior(EMouseLockMode::DoNotLock);
		SetInputMode(Mode);
	}
	else
	{
		SetInputMode(FInputModeGameOnly());
	}
}

void AOUPlayerMenuController::OnLeftClick()
{
	float MX = 0.f, MY = 0.f;
	if (!GetMousePosition(MX, MY))
	{
		return;
	}
	const FVector2D Mouse(MX, MY);
	UE_LOG(LogOUMenu, Log, TEXT("Клик в меню: %.0f,%.0f (экран %d)"),
		Mouse.X, Mouse.Y, static_cast<int32>(Screen));

	for (const FButtonHit& B : Buttons)
	{
		if (Mouse.X >= B.Pos.X && Mouse.X <= B.Pos.X + B.Size.X &&
			Mouse.Y >= B.Pos.Y && Mouse.Y <= B.Pos.Y + B.Size.Y)
		{
			// Пауза-меню и его настройки обрабатываем первыми.
			if (bPauseOpen)
			{
				if (Screen == EOUMenuScreen::Settings)
				{
					AOUCharacterPawn* Char = Cast<AOUCharacterPawn>(GetPawn());
					if (B.Id == 10 && Char)
					{
						Char->MouseSensitivity = FMath::Clamp(Char->MouseSensitivity - 0.0005f, 0.0005f, 0.02f);
					}
					else if (B.Id == 11 && Char)
					{
						Char->MouseSensitivity = FMath::Clamp(Char->MouseSensitivity + 0.0005f, 0.0005f, 0.02f);
					}
					else if (B.Id == 12)
					{
						Screen = EOUMenuScreen::Pause;
					}
				}
				else
				{
					if (B.Id == 1)
					{
						ClosePauseMenu();
					}
					else if (B.Id == 2)
					{
						Screen = EOUMenuScreen::Settings;
					}
					else if (B.Id == 3)
					{
						LeaveServer();
					}
				}
				return;
			}
			if (Screen == EOUMenuScreen::Character)
			{
				if (B.Id == 1)
				{
					// Далее: к выбору сервера.
					if (Profile.Name.Len() >= 2)
					{
						Screen = EOUMenuScreen::Server;
					}
					else
					{
						SetConnectStatus(TEXT("Позывной — минимум 2 символа."));
					}
				}
			}
			else if (Screen == EOUMenuScreen::Server)
			{
				if (B.Id >= 100 && B.Id < 100 + Servers.Num())
				{
					SelectedServer = B.Id - 100;
				}
				else if (B.Id == 1)
				{
					ConnectToSelected();
				}
				else if (B.Id == 2)
				{
					Screen = EOUMenuScreen::Character;
				}
			}
			return;
		}
	}
}

void AOUPlayerMenuController::OnTextInput(const FString& Text)
{
	if (Screen == EOUMenuScreen::Server)
	{
		// Печатаем адрес сервера вручную.
		for (TCHAR C : Text)
		{
			if ((FChar::IsAlnum(C) || C == TEXT('.') || C == TEXT('-') || C == TEXT(':')) && ManualAddress.Len() < 64)
			{
				ManualAddress.AppendChar(C);
			}
		}
		return;
	}
	if (Screen != EOUMenuScreen::Character)
	{
		return;
	}
	// Только осмысленные символы: буквы, цифры, пробел, дефис, точка.
	for (TCHAR C : Text)
	{
		if (FChar::IsAlnum(C) || C == TEXT(' ') || C == TEXT('-') || C == TEXT('_') || C == TEXT('.'))
		{
			if (Profile.Name.Len() < 24)
			{
				Profile.Name.AppendChar(C);
			}
		}
	}
}

void AOUPlayerMenuController::OnBackspace()
{
	if (Screen == EOUMenuScreen::Character && Profile.Name.Len() > 0)
	{
		Profile.Name.LeftChopInline(1);
	}
}

bool AOUPlayerMenuController::InputKey(const FInputKeyParams& Params)
{
	const FKey Key = Params.Key;

	// --- ESC: пауза-меню в игре ---
	if (Key == EKeys::Escape && Params.Event == IE_Pressed)
	{
		if (bPauseOpen)
		{
			if (Screen == EOUMenuScreen::Settings)
			{
				Screen = EOUMenuScreen::Pause;   // из настроек — назад в паузу
			}
			else
			{
				ClosePauseMenu();                // из паузы — в игру
			}
		}
		else if (!bMenuOpen)
		{
			OpenPauseMenu();                     // в игре — открыть паузу
		}
		return true;
	}

	// Пока открыто стартовое меню — перехватываем ввод текста (позывной/адрес).
	if (bMenuOpen)
	{
		if (Params.Event == IE_Pressed || Params.Event == IE_Repeat)
		{
			if (Key == EKeys::BackSpace)
			{
				if (Screen == EOUMenuScreen::Character)
				{
					OnBackspace();
				}
				else if (Screen == EOUMenuScreen::Server && ManualAddress.Len() > 0)
				{
					ManualAddress.LeftChopInline(1);
				}
				return true;
			}
			if (Key == EKeys::Enter)
			{
				if (Screen == EOUMenuScreen::Character && Profile.Name.Len() >= 2)
				{
					Screen = EOUMenuScreen::Server;
				}
				else if (Screen == EOUMenuScreen::Server)
				{
					ConnectToSelected();
				}
				return true;
			}

			// Печатные символы: буквы, цифры и несколько знаков.
			const TCHAR Ch = KeyToChar(Key);
			if (Ch != 0)
			{
				OnTextInput(FString::Chr(Ch));
				return true;
			}
		}
	}
	return Super::InputKey(Params);
}

TCHAR AOUPlayerMenuController::KeyToChar(const FKey& Key)
{
	// Буквы A–Z.
	static const FKey Letters[26] = {
		EKeys::A, EKeys::B, EKeys::C, EKeys::D, EKeys::E, EKeys::F, EKeys::G,
		EKeys::H, EKeys::I, EKeys::J, EKeys::K, EKeys::L, EKeys::M, EKeys::N,
		EKeys::O, EKeys::P, EKeys::Q, EKeys::R, EKeys::S, EKeys::T, EKeys::U,
		EKeys::V, EKeys::W, EKeys::X, EKeys::Y, EKeys::Z
	};
	for (int32 i = 0; i < 26; ++i)
	{
		if (Key == Letters[i])
		{
			return static_cast<TCHAR>(TEXT('a') + i);
		}
	}
	// Цифры 0–9.
	static const FKey Digits[10] = {
		EKeys::Zero, EKeys::One, EKeys::Two, EKeys::Three, EKeys::Four,
		EKeys::Five, EKeys::Six, EKeys::Seven, EKeys::Eight, EKeys::Nine
	};
	for (int32 i = 0; i < 10; ++i)
	{
		if (Key == Digits[i])
		{
			return static_cast<TCHAR>(TEXT('0') + i);
		}
	}
	if (Key == EKeys::SpaceBar) return TEXT(' ');
	if (Key == EKeys::Period) return TEXT('.');
	if (Key == EKeys::Hyphen) return TEXT('-');
	if (Key == EKeys::Underscore) return TEXT('_');
	if (Key == EKeys::Colon) return TEXT(':');
	return 0;
}

void AOUPlayerMenuController::ConnectToSelected()
{
	FString Address = ManualAddress.TrimStartAndEnd();
	int32 Port = 7777;
	if (Address.IsEmpty() && Servers.IsValidIndex(SelectedServer))
	{
		Address = Servers[SelectedServer].Address;
		Port = Servers[SelectedServer].Port;
	}
	if (Address.IsEmpty())
	{
		SetConnectStatus(TEXT("Введите адрес сервера."));
		return;
	}
	ConnectToAddress(Address, Port);
}

bool AOUPlayerMenuController::ConnectToAddress(const FString& Address, int32 Port)
{
	ResolveNet();
	if (!Net)
	{
		SetConnectStatus(TEXT("Корабль ещё не создан — подождите."));
		return false;
	}
	if (Address.IsEmpty())
	{
		return false;
	}

	// Смена адреса (например, перебор серверов): закрываем старый сокет, иначе
	// Connect() решит, что уже подключён, и не обновит endpoint.
	if (Net->ServerAddress != Address || Net->ServerPort != Port)
	{
		Net->Disconnect();
	}

	Net->ServerAddress = Address;
	Net->ServerPort = Port;
	Net->PlayerName = Profile.Name;
	bConnecting = true;
	SetConnectStatus(FString::Printf(TEXT("Подключаюсь к %s:%d…"), *Address, Port));

	if (!Net->Connect())
	{
		bConnecting = false;
		SetConnectStatus(FString::Printf(TEXT("Не удалось открыть сокет для %s:%d."), *Address, Port));
		return false;
	}
	return true;
}

bool AOUPlayerMenuController::ConnectToServerIndex(int32 Index)
{
	if (!Servers.IsValidIndex(Index))
	{
		return false;
	}
	SelectedServer = Index;
	return ConnectToAddress(Servers[Index].Address, Servers[Index].Port);
}

void AOUPlayerMenuController::StartAutoConnect()
{
	AutoConnectIndex = -1;
	bAwaitingWelcome = false;
	UE_LOG(LogOUMenu, Log, TEXT("Авто-подключение: перебираю %d серверов"), Servers.Num());
}

void AOUPlayerMenuController::AutoConnectStep()
{
	ResolveNet();
	UWorld* World = GetWorld();
	const float Now = World ? World->GetTimeSeconds() : 0.f;

	// Опрашиваем сеть прямо из меню: корабль в это время стоит и не тикает
	// сетью, поэтому ответ сервера (welcome/снимок) нужно забрать здесь.
	if (Net)
	{
		Net->Poll();
	}

	// Если уже подключились (пришёл welcome) — закрываем меню и играем.
	if (Net && Net->IsConnected())
	{
		bMenuOpen = false;
		Screen = EOUMenuScreen::InGame;
		bConnecting = false;
		SetMenuCursor(false);
		UE_LOG(LogOUMenu, Log, TEXT("Авто-подключение: подключено к %s:%d (сущность %d)"),
			*Net->ServerAddress, Net->ServerPort, Net->OwnEntityId);

		// Тестовый авто-выход из корабля (проверка ходьбы).
		if (bAutoExit && !bAutoExitDone)
		{
			bAutoExitDone = true;
			if (APawn* P = GetPawn())
			{
				if (AOUShipPawn* ShipPawn = Cast<AOUShipPawn>(P))
				{
					ShipPawn->ExitShip();
					UE_LOG(LogOUMenu, Log, TEXT("Авто-выход: персонаж вышел из корабля"));
				}
			}
		}
		return;
	}

	// Ждём ответа от текущего сервера.
	if (bAwaitingWelcome)
	{
		if (Now < AutoConnectDeadline)
		{
			return; // ещё ждём
		}
		// Тайм-аут: этот сервер не ответил, пробуем следующий.
		UE_LOG(LogOUMenu, Warning, TEXT("Авто-подключение: сервер %d не ответил, пробую следующий"), AutoConnectIndex);
		bAwaitingWelcome = false;
	}

	// Берём следующий сервер из списка.
	AutoConnectIndex++;
	if (AutoConnectIndex >= Servers.Num())
	{
		// Все серверы перебрали — начинаем заново (ждём появления сервера).
		AutoConnectIndex = 0;
	}
	if (Servers.Num() == 0)
	{
		return;
	}

	if (!Servers.IsValidIndex(AutoConnectIndex))
	{
		AutoConnectIndex = 0;
	}
	const FOUServerInfo& S = Servers[AutoConnectIndex];
	SetConnectStatus(FString::Printf(TEXT("Авто-подключение: %s:%d…"), *S.Address, S.Port));
	if (ConnectToAddress(S.Address, S.Port))
	{
		bAwaitingWelcome = true;
		AutoConnectDeadline = Now + AutoConnectTimeoutSec;
	}
}

void AOUPlayerMenuController::DrawMenu(UCanvas* InCanvas)
{
	if (!bMenuOpen || !InCanvas)
	{
		return;
	}
	// Диагностика: печатаем один раз, чтобы в логе было видно, что меню рисуется.
	if (!bLoggedFirstDraw)
	{
		bLoggedFirstDraw = true;
		UE_LOG(LogOUMenu, Log, TEXT("Меню нарисовано: экран %d, canvas %dx%d"),
			static_cast<int32>(Screen), static_cast<int32>(InCanvas->SizeX), static_cast<int32>(InCanvas->SizeY));
	}
	Buttons.Reset();

	const float W = InCanvas->SizeX;
	const float H = InCanvas->SizeY;

	// Затемнение фона.
	FillBox(InCanvas, FVector2D(0, 0), FVector2D(W, H), ColBg);

	UFont* Font = GEngine->GetLargeFont();
	UFont* Small = GEngine->GetMediumFont();

	const float PanelW = FMath::Min(760.f, W - 80.f);
	const float PanelH = FMath::Min(520.f, H - 120.f);
	const FVector2D PanelPos((W - PanelW) * 0.5f, (H - PanelH) * 0.5f);
	// Все панели — «стеклянные», с акцентной кромкой.
	GlassPanel(InCanvas, PanelPos, FVector2D(PanelW, PanelH), ColPanelBorder, ColAccent);

	DrawText(InCanvas, Font, TEXT("OPEN UNIVERSE"), FVector2D(PanelPos.X + 32, PanelPos.Y + 24), ColAccent);

	// Полоса статуса сервер+клиент (с сайта).
	const FString HealthText = !Health.bKnown
		? TEXT("Статус сервер+клиент: проверяется…")
		: (Health.bOk
			? FString::Printf(TEXT("Сервер+клиент: РАБОТАЮТ  (тик %d, задержка %.0f мс)"), Health.Tick, Health.LatencyMs)
			: FString::Printf(TEXT("Сервер+клиент: ЕСТЬ ПРОБЛЕМЫ  %s"), *Health.Notes));
	const FLinearColor HealthColor = !Health.bKnown ? ColMuted : (Health.bOk ? ColOk : ColErr);
	DrawText(InCanvas, Small, HealthText, FVector2D(PanelPos.X + 32, PanelPos.Y + 66), HealthColor);

	// Разделительная линия.
	FillBox(InCanvas, FVector2D(PanelPos.X + 32, PanelPos.Y + 96), FVector2D(PanelW - 64, 1.f), ColPanelBorder);

	if (Screen == EOUMenuScreen::Character)
	{
		DrawText(InCanvas, Font, TEXT("Создайте персонажа"), FVector2D(PanelPos.X + 32, PanelPos.Y + 116), ColText);
		DrawText(InCanvas, Small, TEXT("Позывной пилота:"), FVector2D(PanelPos.X + 32, PanelPos.Y + 168), ColMuted);

		// Поле ввода позывного.
		const FVector2D FieldPos(PanelPos.X + 32, PanelPos.Y + 194);
		const FVector2D FieldSize(PanelW - 64, 48);
		FillBox(InCanvas, FieldPos, FieldSize, FLinearColor(0.05f, 0.07f, 0.16f, 1.f));
		StrokeBox(InCanvas, FieldPos, FieldSize, ColAccent, 1.5f);
		const FString Shown = Profile.Name.IsEmpty() ? TEXT("_") : Profile.Name + TEXT("_");
		DrawText(InCanvas, Font, Shown, FieldPos + FVector2D(14, 12), ColText);

		// Кнопка «Далее».
		const FVector2D BtnPos(PanelPos.X + 32, PanelPos.Y + PanelH - 84);
		const FVector2D BtnSize(240, 52);
		FillBox(InCanvas, BtnPos, BtnSize, ColAccent);
		DrawText(InCanvas, Font, TEXT("Далее  >"), BtnPos + FVector2D(24, 14), FLinearColor(0.02f, 0.06f, 0.1f, 1.f));
		Buttons.Add({ BtnPos, BtnSize, 1 });

		DrawText(InCanvas, Small, TEXT("Печатайте позывной с клавиатуры, затем нажмите «Далее»."),
			FVector2D(PanelPos.X + 32, PanelPos.Y + PanelH - 22), ColMuted);
	}
	else if (Screen == EOUMenuScreen::Server)
	{
		DrawText(InCanvas, Font, TEXT("Выберите сервер"), FVector2D(PanelPos.X + 32, PanelPos.Y + 116), ColText);

		float Y = PanelPos.Y + 160;
		for (int32 i = 0; i < Servers.Num(); ++i)
		{
			const FOUServerInfo& S = Servers[i];
			const FVector2D RowPos(PanelPos.X + 32, Y);
			const FVector2D RowSize(PanelW - 64, 48);
			const bool bSel = (i == SelectedServer);
			FillBox(InCanvas, RowPos, RowSize, bSel ? FLinearColor(0.13f, 0.18f, 0.38f, 1.f) : FLinearColor(0.05f, 0.07f, 0.16f, 1.f));
			StrokeBox(InCanvas, RowPos, RowSize, bSel ? ColAccent2 : ColPanelBorder, bSel ? 2.f : 1.f);
			const FString Row = FString::Printf(TEXT("%s  -  %s:%d"), *S.Label, *S.Address, S.Port);
			DrawText(InCanvas, Small, Row, RowPos + FVector2D(14, 12), ColText);
			Buttons.Add({ RowPos, RowSize, 100 + i });
			Y += 58;
		}

		// Ручной ввод адреса.
		DrawText(InCanvas, Small, TEXT("Или адрес вручную:"), FVector2D(PanelPos.X + 32, Y + 6), ColMuted);
		const FVector2D AddrPos(PanelPos.X + 32, Y + 32);
		const FVector2D AddrSize(PanelW - 64, 40);
		FillBox(InCanvas, AddrPos, AddrSize, FLinearColor(0.05f, 0.07f, 0.16f, 1.f));
		StrokeBox(InCanvas, AddrPos, AddrSize, ColPanelBorder, 1.f);
		DrawText(InCanvas, Small, ManualAddress.IsEmpty() ? TEXT("- из списка -") : ManualAddress,
			AddrPos + FVector2D(12, 8), ColText);

		// Кнопки.
		const FVector2D ConnectPos(PanelPos.X + 32, PanelPos.Y + PanelH - 84);
		const FVector2D ConnectSize(240, 52);
		FillBox(InCanvas, ConnectPos, ConnectSize, ColAccent2);
		DrawText(InCanvas, Font, TEXT("Подключиться"), ConnectPos + FVector2D(24, 14), FLinearColor(0.02f, 0.06f, 0.1f, 1.f));
		Buttons.Add({ ConnectPos, ConnectSize, 1 });

		const FVector2D BackPos(PanelPos.X + 32 + 256, PanelPos.Y + PanelH - 84);
		const FVector2D BackSize(160, 52);
		FillBox(InCanvas, BackPos, BackSize, FLinearColor(0.10f, 0.13f, 0.28f, 1.f));
		DrawText(InCanvas, Small, TEXT("< Назад"), BackPos + FVector2D(30, 14), ColText);
		Buttons.Add({ BackPos, BackSize, 2 });

		if (!ConnectStatus.IsEmpty())
		{
			DrawText(InCanvas, Small, ConnectStatus, FVector2D(PanelPos.X + 32, PanelPos.Y + PanelH - 22), ColMuted);
		}
	}

	DrawText(InCanvas, Small, FString::Printf(TEXT("Пилот: %s"), *Profile.Name),
		FVector2D(PanelPos.X + PanelW - 260, PanelPos.Y + 30), ColMuted);
}

void AOUPlayerMenuController::DrawBuildMenu(UCanvas* InCanvas)
{
	if (bMenuOpen || bPauseOpen || !InCanvas)   // в игре и есть канвас
	{
		return;
	}
	// Меню строительства живёт в персонаже; ищем его по текущей пешке.
	AOUCharacterPawn* Char = Cast<AOUCharacterPawn>(GetPawn());
	if (!Char || !Char->bBuildMenuOpen)
	{
		return;
	}

	const float W = InCanvas->SizeX;
	const float H = InCanvas->SizeY;
	UFont* Font = GEngine->GetLargeFont();
	UFont* Small = GEngine->GetMediumFont();

	// Прицел в центре экрана: показывает, что ЛКМ/Е/R работают «по направлению
	// взгляда». Рисуем маленький крест.
	{
		const float CX = W * 0.5f, CY = H * 0.5f, S = 7.f;
		FillBox(InCanvas, FVector2D(CX - S, CY - 1.f), FVector2D(S * 2.f, 2.f), ColAccent2);
		FillBox(InCanvas, FVector2D(CX - 1.f, CY - S), FVector2D(2.f, S * 2.f), ColAccent2);
	}

	// Панель палитры внизу экрана.
	const int32 Count = (int32)EOUBuildPart::Count;
	const float SlotW = 150.f, SlotH = 90.f, Gap = 12.f;
	const float TotalW = Count * SlotW + (Count - 1) * Gap;
	const FVector2D PanelPos((W - (TotalW + 48.f)) * 0.5f, H - SlotH - 84.f);
	const FVector2D PanelSize(TotalW + 48.f, SlotH + 70.f);
	GlassPanel(InCanvas, PanelPos, PanelSize, ColPanelBorder, ColAccent);

	DrawText(InCanvas, Font, TEXT("Строительство"), PanelPos + FVector2D(24, 14), ColAccent);

	// Слоты: цвет блока, название, подсветка выбранного.
	float X = PanelPos.X + 24;
	const float SlotY = PanelPos.Y + 58;
	for (int32 i = 0; i < Count; ++i)
	{
		const FVector2D SlotPos(X, SlotY);
		const FVector2D SlotSize(SlotW, SlotH);
		const FLinearColor PartCol = AOUCharacterPawn::BuildPartColor(i);
		const bool bSel = (i == Char->BuildSelection);
		FillBox(InCanvas, SlotPos, SlotSize, bSel ? FLinearColor(0.16f, 0.22f, 0.44f, 1.f) : FLinearColor(0.05f, 0.07f, 0.16f, 1.f));
		StrokeBox(InCanvas, SlotPos, SlotSize, bSel ? ColAccent2 : ColPanelBorder, bSel ? 3.f : 1.f);
		// Образец цвета блока.
		FillBox(InCanvas, SlotPos + FVector2D(12, 12), FVector2D(SlotW - 24, 34), PartCol);
		DrawText(InCanvas, Small, AOUCharacterPawn::BuildPartName(i), SlotPos + FVector2D(12, 54), ColText);
		X += SlotW + Gap;
	}

	DrawText(InCanvas, Small, TEXT("B — закрыть   ·   ЛКМ — поставить   ·   E — удалить   ·   R — покрасить   ·   колесо — выбор блока"),
		FVector2D(PanelPos.X + 24, PanelPos.Y + PanelSize.Y + 8), ColMuted);

	DrawText(InCanvas, Small, FString::Printf(TEXT("Блоков поставлено: %d"), Char->BuiltCount),
		FVector2D(PanelPos.X + PanelSize.X - 220, PanelPos.Y + 16), ColOk);
}

void AOUPlayerMenuController::OpenPauseMenu()
{
	if (bMenuOpen)
	{
		return;   // стартовое меню ещё открыто — пауза не нужна
	}
	bPauseOpen = true;
	// Онлайн-мир не «замирает» (сервер продолжает работать), но локально мы
	// освобождаем курсор и глушим управление персонажем (см. AOUCharacterPawn::Tick).
	SetMenuCursor(true);
	UE_LOG(LogOUMenu, Log, TEXT("Пауза-меню открыто (ESC)"));
}

void AOUPlayerMenuController::ClosePauseMenu()
{
	bPauseOpen = false;
	// Настройки при закрытии паузы тоже схлопываем.
	if (Screen == EOUMenuScreen::Settings)
	{
		Screen = EOUMenuScreen::Pause;
	}
	SetMenuCursor(false);
	UE_LOG(LogOUMenu, Log, TEXT("Пауза-меню закрыто — возврат в игру"));
}

void AOUPlayerMenuController::LeaveServer()
{
	// Отлучиться: рвём соединение и возвращаемся в стартовое меню выбора сервера.
	bPauseOpen = false;
	ResolveNet();
	if (Net)
	{
		Net->Disconnect();
	}
	bConnecting = false;
	bAutoConnectStarted = false;
	Screen = EOUMenuScreen::Server;
	bMenuOpen = true;
	SetConnectStatus(TEXT("Отлучились от сервера."));
	SetMenuCursor(true);
	UE_LOG(LogOUMenu, Log, TEXT("Отлучились от сервера — вернулись в меню выбора сервера"));
}

void AOUPlayerMenuController::DrawPauseMenu(UCanvas* InCanvas)
{
	if (!bPauseOpen || !InCanvas)
	{
		return;
	}
	Buttons.Reset();

	const float W = InCanvas->SizeX;
	const float H = InCanvas->SizeY;
	UFont* Font = GEngine->GetLargeFont();
	UFont* Small = GEngine->GetMediumFont();

	// Затемнение фона (полупрозрачное — видно игру).
	FillBox(InCanvas, FVector2D(0, 0), FVector2D(W, H), FLinearColor(0.02f, 0.03f, 0.06f, 0.72f));

	const float PanelW = FMath::Min(520.f, W - 80.f);
	const float PanelH = FMath::Min(420.f, H - 120.f);
	const FVector2D PanelPos((W - PanelW) * 0.5f, (H - PanelH) * 0.5f);
	GlassPanel(InCanvas, PanelPos, FVector2D(PanelW, PanelH), ColPanelBorder, ColAccent);

	DrawText(InCanvas, Font, TEXT("ПАУЗА"), FVector2D(PanelPos.X + 28, PanelPos.Y + 22), ColAccent);

	// --- Экран настроек ---
	if (Screen == EOUMenuScreen::Settings)
	{
		AOUCharacterPawn* Char = Cast<AOUCharacterPawn>(GetPawn());
		const float Sens = Char ? Char->MouseSensitivity : 0.0025f;

		DrawText(InCanvas, Small, FString::Printf(TEXT("Чувствительность мыши: %.4f"), Sens),
			FVector2D(PanelPos.X + 28, PanelPos.Y + 88), ColText);

		const float SBX = PanelPos.X + 28;
		const float SBW = PanelW - 56;
		// Кнопки «-» и «+».
		const FVector2D MinusPos(SBX, PanelPos.Y + 120);
		const FVector2D PlusPos(SBX + SBW - 64, PanelPos.Y + 120);
		const FVector2D StepSize(64, 48);
		FillBox(InCanvas, MinusPos, StepSize, FLinearColor(0.10f, 0.13f, 0.28f, 1.f));
		StrokeBox(InCanvas, MinusPos, StepSize, ColPanelBorder, 1.5f);
		DrawText(InCanvas, Font, TEXT("-"), MinusPos + FVector2D(22, 8), ColText);
		Buttons.Add({ MinusPos, StepSize, 10 });

		FillBox(InCanvas, PlusPos, StepSize, FLinearColor(0.10f, 0.13f, 0.28f, 1.f));
		StrokeBox(InCanvas, PlusPos, StepSize, ColPanelBorder, 1.5f);
		DrawText(InCanvas, Font, TEXT("+"), PlusPos + FVector2D(20, 8), ColText);
		Buttons.Add({ PlusPos, StepSize, 11 });

		// Полоса-индикатор чувствительности (0.005..0.05).
		const FVector2D BarPos(SBX + 76, PanelPos.Y + 134);
		const FVector2D BarSize(SBW - 152, 20);
		Bar(InCanvas, BarPos, BarSize, (Sens - 0.0005f) / 0.0195f, ColAccent2,
			FLinearColor(0.05f, 0.07f, 0.16f, 1.f));

		// Кнопка «Назад» в паузу.
		const FVector2D BackPos(SBX, PanelPos.Y + PanelH - 84);
		const FVector2D BackSize(200, 52);
		FillBox(InCanvas, BackPos, BackSize, FLinearColor(0.10f, 0.13f, 0.28f, 1.f));
		DrawText(InCanvas, Font, TEXT("< Назад"), BackPos + FVector2D(28, 14), ColText);
		Buttons.Add({ BackPos, BackSize, 12 });

		DrawText(InCanvas, Small, TEXT("ESC — назад в паузу"),
			FVector2D(PanelPos.X + 28, PanelPos.Y + PanelH - 28), ColMuted);
		return;
	}

	// --- Главный экран паузы ---
	// Строки меню: id 1 — продолжить, 2 — настройки, 3 — отлучиться.
	const float BtnX = PanelPos.X + 28;
	const float BtnW = PanelW - 56;
	const float BtnH = 56;
	float BY = PanelPos.Y + 96;

	auto MenuRow = [&](int32 Id, const TCHAR* Label, const FLinearColor& Accent)
	{
		const FVector2D RowPos(BtnX, BY);
		const FVector2D RowSize(BtnW, BtnH);
		FillBox(InCanvas, RowPos, RowSize, FLinearColor(0.07f, 0.09f, 0.19f, 0.96f));
		StrokeBox(InCanvas, RowPos, RowSize, Accent, 1.5f);
		FillBox(InCanvas, RowPos, FVector2D(4.f, RowSize.Y), Accent);
		DrawText(InCanvas, Font, Label, RowPos + FVector2D(20, 16), ColText);
		Buttons.Add({ RowPos, RowSize, Id });
		BY += BtnH + 12;
	};
	MenuRow(1, TEXT("Продолжить"), ColAccent2);
	MenuRow(2, TEXT("Настройки"), ColAccent);
	MenuRow(3, TEXT("Отлучиться от сервера"), ColErr);

	DrawText(InCanvas, Small, TEXT("ESC — закрыть  ·  ЛКМ — выбрать"),
		FVector2D(PanelPos.X + 28, PanelPos.Y + PanelH - 28), ColMuted);
}

void AOUPlayerMenuController::DrawMarket(UCanvas* InCanvas)
{
	if (!bMarketOpen || !InCanvas || bMenuOpen)
	{
		return;
	}
	ResolveNet();

	const float W = InCanvas->SizeX;
	const float H = InCanvas->SizeY;
	UFont* Font = GEngine->GetLargeFont();
	UFont* Small = GEngine->GetMediumFont();
	UFont* Tiny = GEngine->GetSmallFont();
	const FLinearColor PanelB(0.34f, 0.44f, 0.85f, 0.9f);
	const FLinearColor Dim(0.66f, 0.72f, 0.86f, 1.f);

	const float PW = FMath::Min(900.f, W - 80.f);
	const float PH = FMath::Min(600.f, H - 100.f);
	const FVector2D P((W - PW) * 0.5f, (H - PH) * 0.5f);
	GlassPanel(InCanvas, P, FVector2D(PW, PH), PanelB, ColAccent2);
	DrawText(InCanvas, Font, TEXT("РЫНОК  ·  HOME"), P + FVector2D(24, 16), ColAccent);
	DrawText(InCanvas, Small, TEXT("M — закрыть"), P + FVector2D(PW - 140, 24), Dim);

	// Кредиты.
	const int64 Credits = Net ? Net->Credits : 0;
	DrawText(InCanvas, Font, FString::Printf(TEXT("Кредиты: %lld"), Credits), P + FVector2D(24, 56), ColOk);

	// Инвентарь (левая колонка).
	DrawText(InCanvas, Small, TEXT("Инвентарь"), P + FVector2D(24, 96), ColText);
	float Y = P.Y + 122;
	if (Net && Net->InventoryItems.Num() > 0)
	{
		for (const FOUInventoryItem& It : Net->InventoryItems)
		{
			const FString Row = FString::Printf(TEXT("%s  ×%lld"), *ItemName(It.Item), It.Qty);
			DrawText(InCanvas, Small, Row, FVector2D(P.X + 24, Y), Dim);
			Y += 22;
		}
	}
	else
	{
		DrawText(InCanvas, Small, TEXT("(пусто — копайте грунт)"), FVector2D(P.X + 24, Y), Dim);
	}

	// Стакан (правая колонка).
	const float RX = P.X + PW * 0.5f;
	DrawText(InCanvas, Small, TEXT("Стакан: покупки / продажи"), FVector2D(RX, P.Y + 96), ColText);
	float BY = P.Y + 122;
	auto DrawLevels = [&](const TArray<FOUOrderLevel>& Lv, const FLinearColor& C, const TCHAR* Tag)
	{
		if (Lv.Num() == 0)
		{
			DrawText(InCanvas, Tiny, FString::Printf(TEXT("%s: —"), Tag), FVector2D(RX, BY), Dim);
			BY += 20;
			return;
		}
		for (const FOUOrderLevel& L : Lv)
		{
			const FString Row = FString::Printf(TEXT("%s %s  %lld ×%lld"), Tag, *ItemName(L.Item), L.Price, L.Qty);
			DrawText(InCanvas, Tiny, Row, FVector2D(RX, BY), C);
			BY += 18;
		}
		BY += 6;
	};
	DrawLevels(Net ? Net->MarketBids : TArray<FOUOrderLevel>(), ColOk, TEXT("ПОК"));
	DrawLevels(Net ? Net->MarketAsks : TArray<FOUOrderLevel>(), FLinearColor(1.f, 0.6f, 0.4f), TEXT("ПРО"));

	// Подсказка по торговле + простая горячая сделка: продать руду, купить руду.
	DrawText(InCanvas, Small,
		TEXT("1 — продать 1 руду по 20   ·   2 — купить 1 руду по 20   ·   (позже: выбор цены)"),
		P + FVector2D(24, PH - 52), Dim);
}

FString AOUPlayerMenuController::ItemName(int32 Item)
{
	// Имена предметов дублируют серверный `ItemRegistry::default_universe`.
	switch (Item)
	{
	case 1: return TEXT("руда");
	case 2: return TEXT("лёд");
	case 3: return TEXT("кристалл");
	case 4: return TEXT("металл");
	case 5: return TEXT("топливо");
	case 6: return TEXT("сплав");
	case 7: return TEXT("электроника");
	case 8: return TEXT("корпус");
	case 9: return TEXT("двигатель");
	case 10: return TEXT("корабль");
	default: return FString::Printf(TEXT("предмет %d"), Item);
	}
}

void AOUPlayerMenuController::DrawGameHud(UCanvas* InCanvas)
{
	// Постоянный HUD в духе Dual Universe: тонкие линии, «стеклянные» панели,
	// статусы по углам, компас, мини-карта, прицел и хотбар. Рисуется всегда,
	// когда игровое меню закрыто.
	if (bMenuOpen || !InCanvas)
	{
		return;
	}

	const float W = InCanvas->SizeX;
	const float H = InCanvas->SizeY;
	UFont* Small = GEngine->GetMediumFont();
	UFont* Tiny = GEngine->GetSmallFont();

	const FLinearColor Line(0.42f, 0.55f, 1.0f, 0.9f);   // акцентная линия
	const FLinearColor Dim(0.66f, 0.72f, 0.86f, 1.f);
	const FLinearColor PanelB(0.34f, 0.44f, 0.85f, 0.9f);

	APawn* MyPawn = GetPawn();
	const FVector Pos = MyPawn ? MyPawn->GetActorLocation() : FVector::ZeroVector;
	const FVector Up = Pos.IsNearlyZero() ? FVector::UpVector : Pos.GetSafeNormal();
	const float Alt = Pos.Size() / 100.f - 500000.f;   // над уровнем океана, м

	// --- Прицел: четыре штриха + центральная точка (не перекрывает обзор) ---
	{
		const float CX = W * 0.5f, CY = H * 0.5f, G = 7.f, L = 9.f;
		const FLinearColor A(0.88f, 0.94f, 1.0f, 0.9f);
		FillBox(InCanvas, FVector2D(CX - G - L, CY - 1.f), FVector2D(L, 2.f), A);
		FillBox(InCanvas, FVector2D(CX + G, CY - 1.f), FVector2D(L, 2.f), A);
		FillBox(InCanvas, FVector2D(CX - 1.f, CY - G - L), FVector2D(2.f, L), A);
		FillBox(InCanvas, FVector2D(CX - 1.f, CY + G), FVector2D(2.f, L), A);
		FillBox(InCanvas, FVector2D(CX - 2.f, CY - 2.f), FVector2D(4.f, 4.f), ColAccent2);
	}

	// --- Верх по центру: компас (стороны света по курсу) ---
	{
		const float CW = 320.f, CH = 30.f;
		const FVector2D CP((W - CW) * 0.5f, 14.f);
		FillBox(InCanvas, CP, FVector2D(CW, CH), FLinearColor(0.03f, 0.05f, 0.10f, 0.55f));
		StrokeBox(InCanvas, CP, FVector2D(CW, CH), PanelB, 1.f);
		// Курс: проекция «вперёд» на касательную плоскость.
		FVector Fwd = MyPawn ? MyPawn->GetActorForwardVector() : FVector::ForwardVector;
		Fwd = FVector::VectorPlaneProject(Fwd, Up).GetSafeNormal();
		FVector Ref = FVector::CrossProduct(Up, FVector::UpVector);
		if (Ref.SizeSquared() < 1e-4f) { Ref = FVector::CrossProduct(Up, FVector::RightVector); }
		Ref = Ref.GetSafeNormal();
		const FVector Tang = FVector::CrossProduct(Ref, Up).GetSafeNormal();
		const float Heading = FMath::Atan2(FVector::DotProduct(Fwd, FVector::CrossProduct(Up, Tang)), FVector::DotProduct(Fwd, Tang));
		const float Deg = FMath::RadiansToDegrees(Heading);
		// Метки каждые 45° вокруг текущего курса.
		const TCHAR* Marks[8] = { TEXT("С"), TEXT("СВ"), TEXT("В"), TEXT("ЮВ"), TEXT("Ю"), TEXT("ЮЗ"), TEXT("З"), TEXT("СЗ") };
		for (int32 i = 0; i < 8; ++i)
		{
			float Rel = i * 45.f - Deg;
			while (Rel > 180.f) Rel -= 360.f;
			while (Rel < -180.f) Rel += 360.f;
			if (FMath::Abs(Rel) <= 45.f)
			{
				const float MX = CP.X + CW * 0.5f + (Rel / 45.f) * (CW * 0.42f);
				const FLinearColor C = (i % 2 == 0) ? ColText : Dim;
				DrawText(InCanvas, Tiny, Marks[i], FVector2D(MX - 8.f, CP.Y + 7.f), C);
			}
		}
		// Центральный указатель курса.
		FillBox(InCanvas, FVector2D(CP.X + CW * 0.5f - 1.f, CP.Y), FVector2D(2.f, 7.f), ColAccent2);
		DrawText(InCanvas, Tiny, FString::Printf(TEXT("%03.0f°"), Deg < 0.f ? Deg + 360.f : Deg),
			FVector2D(CP.X + CW * 0.5f - 16.f, CP.Y + CH - 4.f), ColOk);
	}

	// --- Слева сверху: место, высота, координаты ---
	if (MyPawn)
	{
		const FVector2D P(16, 16);
		GlassPanel(InCanvas, P, FVector2D(230, 112), PanelB, Line);
		DrawText(InCanvas, Small, TEXT("ПЛАНЕТА  ·  HOME"), P + FVector2D(12, 8), ColText);		DrawText(InCanvas, Tiny, FString::Printf(TEXT("высота  %+6.0f м"), Alt), P + FVector2D(12, 38), Dim);
		DrawText(InCanvas, Tiny, FString::Printf(TEXT("коорд  %.0f, %.0f"), Pos.X / 100.f, Pos.Y / 100.f), P + FVector2D(12, 56), Dim);
		DrawText(InCanvas, Tiny, FString::Printf(TEXT("позывной  %s"), *Profile.Name), P + FVector2D(12, 74), ColAccent);
		if (Net && Net->bMarketReceived)
		{
			DrawText(InCanvas, Tiny, FString::Printf(TEXT("кредиты  %lld"), Net->Credits), P + FVector2D(12, 92), ColOk);
		}
	}

	// --- Справа сверху: состояние сети ---
	ResolveNet();
	{
		const bool bConnected = Net && (Net->IsConnected() || Net->bSystemReceived);
		const FString St = bConnected ? TEXT("СВЯЗЬ  OK") : TEXT("СВЯЗЬ  НЕТ");
		const FLinearColor C = bConnected ? ColOk : ColErr;
		const FVector2D P(W - 210, 16);
		GlassPanel(InCanvas, P, FVector2D(194, 34), PanelB, C);
		DrawText(InCanvas, Small, St, P + FVector2D(12, 8), C);
		int32 Ping = 0;
		if (bConnected && Net) { Ping = FMath::RoundToInt(Health.LatencyMs); }
		DrawText(InCanvas, Tiny, FString::Printf(TEXT("задержка %d мс"), Ping), P + FVector2D(112, 12), Dim);
	}

	// --- Справа снизу: мини-карта (планета, игрок, база, другие) ---
	{
		const float MR = 78.f;
		const FVector2D MC(W - MR - 24.f, H - MR - 24.f);
		// Диск: горизонтальные полосы, обрезанные по кругу (канвас не умеет clip).
		for (int32 i = 0; i < 40; ++i)
		{
			const float T = -1.f + 2.f * (i + 0.5f) / 40.f;
			const float HalfW = MR * FMath::Sqrt(FMath::Max(0.f, 1.f - T * T));
			FillBox(InCanvas, FVector2D(MC.X - HalfW, MC.Y + T * MR), FVector2D(HalfW * 2.f, (2.f * MR) / 40.f + 1.f),
				FLinearColor(0.02f, 0.04f, 0.09f, 0.55f));
		}
		// Кольцо-рамка.
		for (int32 i = 0; i < 56; ++i)
		{
			const float A0 = 6.2831853f * i / 56.f;
			FillBox(InCanvas, MC + FVector2D(FMath::Cos(A0) * MR - 1.f, FMath::Sin(A0) * MR - 1.f), FVector2D(2, 2), PanelB);
		}
		// Точки: игрок (центр, зелёный), база (акцент, «север» карты), другие.
		FillBox(InCanvas, MC - FVector2D(3, 3), FVector2D(6, 6), ColOk);
		FillBox(InCanvas, MC + FVector2D(-4, -44), FVector2D(8, 8), ColAccent2);
		if (Net)
		{
			for (const FOUEntityState& E : Net->LatestSnapshot.Entities)
			{
				if (E.Id == Net->OwnEntityId) { continue; }
				const float RX = FMath::Clamp(E.Position.X / 20000.f, -1.f, 1.f) * (MR - 10.f);
				const float RY = FMath::Clamp(E.Position.Y / 20000.f, -1.f, 1.f) * (MR - 10.f);
				FillBox(InCanvas, MC + FVector2D(RX - 2, RY - 2), FVector2D(4, 4), WarmDot);
			}
		}
		DrawText(InCanvas, Tiny, TEXT("КАРТА"), MC + FVector2D(-18, MR + 4.f), Dim);
	}

	// --- Корабль: приборы (скорость, тяга, стрейф/верх, целостность) ---
	AOUShipPawn* ShipPawn = Cast<AOUShipPawn>(MyPawn);
	if (ShipPawn)
	{
		const FVector2D P(16, H - 132);
		GlassPanel(InCanvas, P, FVector2D(260, 100), PanelB, Line);
		const float Speed = ShipPawn->State.Velocity.Size();
		DrawText(InCanvas, Small, FString::Printf(TEXT("СКОРОСТЬ  %.0f м/с"), Speed), P + FVector2D(12, 8), ColText);
		Bar(InCanvas, P + FVector2D(12, 34), FVector2D(236, 10), Speed / 400.f, ColAccent2, FLinearColor(0.1f, 0.12f, 0.2f, 0.9f));
		if (Net)
		{
			const int32 Sh = Net->OwnHull.Shield, Ar = Net->OwnHull.Armor;
			DrawText(InCanvas, Tiny, TEXT("ЩИТ"), P + FVector2D(12, 52), Dim);
			Bar(InCanvas, P + FVector2D(52, 54), FVector2D(196, 8), Sh / 100.f, FLinearColor(0.3f, 0.7f, 1.f), FLinearColor(0.1f, 0.12f, 0.2f, 0.9f));
			DrawText(InCanvas, Tiny, TEXT("БРОНЯ"), P + FVector2D(12, 68), Dim);
			Bar(InCanvas, P + FVector2D(52, 70), FVector2D(196, 8), Ar / 100.f, FLinearColor(1.f, 0.7f, 0.3f), FLinearColor(0.1f, 0.12f, 0.2f, 0.9f));
		}
	}

	// --- Снизу по центру: хотбар (скрыт при палитре строительства) ---
	const AOUCharacterPawn* Char = Cast<AOUCharacterPawn>(MyPawn);
	const bool bBuilding = Char && Char->bBuildMenuOpen;
	if (!bBuilding)
	{
		const int32 Slots = 5;
		const float SW = 56.f, SH = 44.f, Gap = 8.f;
		const float TotalW = Slots * SW + (Slots - 1) * Gap;
		const float X0 = (W - TotalW) * 0.5f;
		const float Y0 = H - SH - 30.f;
		for (int32 i = 0; i < Slots; ++i)
		{
			const FVector2D P(X0 + i * (SW + Gap), Y0);
			const bool bFirst = (i == 0);
			FillBox(InCanvas, P, FVector2D(SW, SH), FLinearColor(0.05f, 0.07f, 0.14f, 0.72f));
			StrokeBox(InCanvas, P, FVector2D(SW, SH), bFirst ? ColAccent2 : PanelB, bFirst ? 2.f : 1.f);
			DrawText(InCanvas, Tiny, FString::Printf(TEXT("%d"), i + 1), P + FVector2D(7, 5), bFirst ? ColAccent2 : Dim);
		}
	}

	// --- Снизу слева: подсказка управления (своя для пешего и корабля) ---
	if (!bBuilding)
	{
		const bool bInShip = ShipPawn != nullptr;
		const FString Hint = bInShip
			? TEXT("Мышь — обзор   ·   W/S — тяга   ·   A/D — стрейф   ·   Пробел/Ctrl — вверх/вниз   ·   ЛКМ — огонь   ·   F — выход")
			: TEXT("Мышь — обзор   ·   W/A/S/D — ходьба   ·   ЛКМ — копать   ·   M — рынок   ·   B — строительство");
		DrawText(InCanvas, Tiny, Hint, FVector2D(18, H - 22), Dim);
	}
}

