// Copyright Open Universe. Все права защищены.

#include "OUPlayerMenuController.h"
#include "OUClientNet.h"
#include "OUShipPawn.h"
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
	APawn* P = GetPawn();
	if (!P)
	{
		return;
	}
	Net = P->FindComponentByClass<UOUClientNet>();
	if (!Net)
	{
		Net = NewObject<UOUClientNet>(P, TEXT("OUClientNet"));
		Net->RegisterComponent();
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
	// Пока открыто меню — перехватываем ввод текста (позывной/адрес). Игре
	// ввод сейчас не нужен: она ещё не началась.
	if (bMenuOpen)
	{
		const FKey Key = Params.Key;
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
				if (AOUShipPawn* Ship = Cast<AOUShipPawn>(P))
				{
					Ship->ExitShip();
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
	FillBox(InCanvas, PanelPos, FVector2D(PanelW, PanelH), ColPanel);
	StrokeBox(InCanvas, PanelPos, FVector2D(PanelW, PanelH), ColPanelBorder, 2.f);

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
