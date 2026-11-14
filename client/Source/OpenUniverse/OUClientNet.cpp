// Copyright Open Universe. Все права защищены.

#include "OUClientNet.h"
#include "OUWire.h"
#include "Sockets.h"
#include "SocketSubsystem.h"
#include "Interfaces/IPv4/IPv4Address.h"
#include "Interfaces/IPv4/IPv4Endpoint.h"
#include "Common/UdpSocketBuilder.h"
#include "Common/TcpSocketBuilder.h"

DEFINE_LOG_CATEGORY_STATIC(LogOUClient, Log, All);

UOUClientNet::UOUClientNet()
{
	PrimaryComponentTick.bCanEverTick = true;
	PrimaryComponentTick.bStartWithTickEnabled = true;
	// Четыре слоя добычи (руда, лёд, кристалл, металл).
	DigTotalByLayer.Init(0, 4);
}

void UOUClientNet::BeginPlay()
{
	Super::BeginPlay();
	// Подключение запускает меню персонажа/сервера (кнопка «Подключиться»),
	// а не сам компонент: иначе игра стартовала бы сразу, минуя меню.
}

void UOUClientNet::EndPlay(const EEndPlayReason::Type Reason)
{
	Disconnect();
	Super::EndPlay(Reason);
}

bool UOUClientNet::Connect()
{
	if (Socket)
	{
		return true;
	}
	State = EOUConnState::Connecting;

	ISocketSubsystem* SocketSub = ISocketSubsystem::Get(PLATFORM_SOCKETSUBSYSTEM);
	if (!SocketSub)
	{
		State = EOUConnState::Error;
		UE_LOG(LogOUClient, Error, TEXT("Нет подсистемы сокетов"));
		return false;
	}

	// Клиентский UDP-сокет на любом свободном порту.
	Socket = FUdpSocketBuilder(TEXT("OUClient"))
		.AsNonBlocking()
		.Build();
	if (!Socket)
	{
		State = EOUConnState::Error;
		UE_LOG(LogOUClient, Error, TEXT("Не удалось создать сокет"));
		return false;
	}

	// Адрес сервера.
	TSharedPtr<FInternetAddr> ServerAddr = SocketSub->CreateInternetAddr();
	bool bValid = false;
	ServerAddr->SetIp(*ServerAddress, bValid);
	ServerAddr->SetPort(ServerPort);
	if (!bValid)
	{
		State = EOUConnState::Error;
		UE_LOG(LogOUClient, Error, TEXT("Неверный адрес сервера: %s"), *ServerAddress);
		// Освобождаем сокет, чтобы повторный Connect() не считал себя подключённым.
		Disconnect();
		return false;
	}
	ServerEndpoint = ServerAddr;

	if (!SendHello())
	{
		Disconnect();
		return false;
	}

	// Пока не пришёл ответ (welcome/снимок), состояние — «подключение».
	State = EOUConnState::Connecting;
	UE_LOG(LogOUClient, Log, TEXT("Приветствие отправлено на %s:%d"), *ServerAddress, ServerPort);
	return true;
}

bool UOUClientNet::SendHello()
{
	if (!Socket || !ServerEndpoint.IsValid())
	{
		return false;
	}
	// Приветствие (тег 1): [tag][protocol u16][name len u16][name].
	TArray<uint8> Body;
	Body.Add(1);
	Body.Add(1); // protocol lo
	Body.Add(0); // protocol hi
	const FTCHARToUTF8 NameUtf8(*PlayerName);
	const int32 NameLen = NameUtf8.Length();
	Body.Add(static_cast<uint8>(NameLen & 0xFF));
	Body.Add(static_cast<uint8>((NameLen >> 8) & 0xFF));
	for (int32 i = 0; i < NameLen; ++i)
	{
		Body.Add(static_cast<uint8>(NameUtf8.Get()[i]));
	}
	return SendPacket(Body);
}

void UOUClientNet::Disconnect()
{
	if (Socket)
	{
		Socket->Close();
		ISocketSubsystem::Get(PLATFORM_SOCKETSUBSYSTEM)->DestroySocket(Socket);
		Socket = nullptr;
	}
	State = EOUConnState::Disconnected;
}

bool UOUClientNet::SendPacket(const TArray<uint8>& Body)
{
	if (!Socket || !ServerEndpoint.IsValid())
	{
		return false;
	}
	// Пакет: [token u64][len u32 LE][body].
	TArray<uint8> Packet;
	Packet.Reserve(8 + 4 + Body.Num());
	for (int32 i = 0; i < 8; ++i)
	{
		Packet.Add(static_cast<uint8>((static_cast<uint64>(SessionToken) >> (8 * i)) & 0xFF));
	}
	const uint32 Len = static_cast<uint32>(Body.Num());
	for (int32 i = 0; i < 4; ++i)
	{
		Packet.Add(static_cast<uint8>((Len >> (8 * i)) & 0xFF));
	}
	Packet.Append(Body);

	int32 BytesSent = 0;
	return Socket->SendTo(Packet.GetData(), Packet.Num(), BytesSent, *ServerEndpoint) && BytesSent > 0;
}

void UOUClientNet::SendInput(const FOUInputFrame& Input)
{
	if (!IsConnected())
	{
		return;
	}
	// Формат кадра — в OUWire (тот же код проверяет стенд test_wire).
	const std::vector<uint8_t> Body = OUWire::EncodeInput(
		static_cast<uint32_t>(Input.Seq),
		Input.Throttle, Input.Yaw, Input.Pitch, Input.Strafe, Input.Up, Input.bFire, Input.bBoost);
	TArray<uint8> Arr;
	Arr.Append(Body.data(), static_cast<int32>(Body.size()));
	if (SendPacket(Arr))
	{
		// Запоминаем время отправки: сервер вернёт этот seq в снимке, и мы
		// посчитаем настоящий RTT (пинг игрока до сервера).
		if (UWorld* W = GetWorld())
		{
			InputSentTime.Add(static_cast<int32>(Input.Seq), W->GetTimeSeconds());
			// Чистим старые записи, чтобы карта не росла.
			if (InputSentTime.Num() > 256)
			{
				double Oldest = 1e30; int32 OldKey = 0;
				for (const TPair<int32, double>& P : InputSentTime)
				{
					if (P.Value < Oldest) { Oldest = P.Value; OldKey = P.Key; }
				}
				InputSentTime.Remove(OldKey);
			}
		}
	}
}

void UOUClientNet::SendPing()
{
	if (!IsConnected())
	{
		return;
	}
	const uint32_t Nonce = static_cast<uint32_t>(++PingNonce);
	const std::vector<uint8_t> Body = OUWire::EncodePing(Nonce);
	TArray<uint8> Arr;
	Arr.Append(Body.data(), static_cast<int32>(Body.size()));
	if (SendPacket(Arr))
	{
		if (UWorld* W = GetWorld())
		{
			PingSentTime.Add(static_cast<int32>(Nonce), W->GetTimeSeconds());
			if (PingSentTime.Num() > 32)
			{
				double Oldest = 1e30; int32 OldKey = 0;
				for (const TPair<int32, double>& P : PingSentTime)
				{
					if (P.Value < Oldest) { Oldest = P.Value; OldKey = P.Key; }
				}
				PingSentTime.Remove(OldKey);
			}
		}
	}
}

void UOUClientNet::SendCharacter(const FVector& PositionServer, float Yaw, bool bMoving, bool bActive)
{
	if (!IsConnected() || OwnEntityId == 0)
	{
		return;
	}
	// Разбор/сборка провода — в OUWire (тот же код, что проверен стендом).
	// Позиция здесь в СЕРВЕРНЫХ единицах (как хранит корабль), переводим в см UE.
	const FVector InCm = OUInterop::ServerToUE(PositionServer);
	const std::vector<uint8_t> Body = OUWire::EncodeCharacter(
		static_cast<uint32_t>(OwnEntityId),
		static_cast<float>(InCm.X), static_cast<float>(InCm.Y), static_cast<float>(InCm.Z),
		Yaw, bMoving, bActive);
	TArray<uint8> Arr;
	Arr.Append(Body.data(), static_cast<int32>(Body.size()));
	SendPacket(Arr);
}

void UOUClientNet::SendDig(int32 Slot, double X, double Y, double Z, double CosRadius, double Depth)
{
	if (!IsConnected())
	{
		return;
	}
	// Формат — в OUWire (тот же код проверяет стенд test_wire).
	const std::vector<uint8_t> Body = OUWire::EncodeDig(
		static_cast<uint32_t>(Slot), X, Y, Z, CosRadius, Depth);
	TArray<uint8> Arr;
	Arr.Append(Body.data(), static_cast<int32>(Body.size()));
	SendPacket(Arr);
}

void UOUClientNet::SendCraft(int32 RecipeId)
{
	if (!IsConnected())
	{
		return;
	}
	const std::vector<uint8_t> Body = OUWire::EncodeCraft(static_cast<uint32_t>(FMath::Max(0, RecipeId)));
	TArray<uint8> Arr;
	Arr.Append(Body.data(), static_cast<int32>(Body.size()));
	SendPacket(Arr);
}

void UOUClientNet::SendMarketOrder(int32 Side, int32 Item, int64 Price, int64 Qty)
{
	if (!IsConnected())
	{
		return;
	}
	const std::vector<uint8_t> Body = OUWire::EncodeMarketOrder(
		static_cast<uint8_t>(Side & 0xFF),
		static_cast<uint32_t>(Item),
		static_cast<uint64_t>(FMath::Max<int64>(0, Price)),
		static_cast<uint64_t>(FMath::Max<int64>(0, Qty)));
	TArray<uint8> Arr;
	Arr.Append(Body.data(), static_cast<int32>(Body.size()));
	SendPacket(Arr);
}

void UOUClientNet::SendCapture(int32 Id)
{
	if (!IsConnected())
	{
		return;
	}
	const std::vector<uint8_t> Body = OUWire::EncodeCapture(static_cast<uint32_t>(FMath::Max(0, Id)));
	TArray<uint8> Arr;
	Arr.Append(Body.data(), static_cast<int32>(Body.size()));
	SendPacket(Arr);
}

void UOUClientNet::SendSetScript(int32 Template)
{
	if (!IsConnected())
	{
		return;
	}
	const std::vector<uint8_t> Body = OUWire::EncodeSetScript(static_cast<uint32_t>(FMath::Max(0, Template)));
	TArray<uint8> Arr;
	Arr.Append(Body.data(), static_cast<int32>(Body.size()));
	SendPacket(Arr);
}

void UOUClientNet::SendPlaceBlueprint(int32 Id, int32 X, int32 Y, int32 Z, int32 Entity)
{
	if (!IsConnected())
	{
		return;
	}
	const std::vector<uint8_t> Body = OUWire::EncodePlaceBlueprint(
		static_cast<uint32_t>(FMath::Max(0, Id)), X, Y, Z, static_cast<uint32_t>(FMath::Max(0, Entity)));
	TArray<uint8> Arr;
	Arr.Append(Body.data(), static_cast<int32>(Body.size()));
	SendPacket(Arr);
}

void UOUClientNet::UpdateTerritoryAt(double ServerX, double ServerY, double ServerZ)
{
	// Точка игрока в серверных единицах; зоны тоже в серверных.
	const double PX = ServerX, PY = ServerY, PZ = ServerZ;
	int32 BestId = 0;
	bool bPeaceful = false;
	for (const FOUTerritory& T : Territories)
	{
		const double DX = PX - T.Center.X;
		const double DY = PY - T.Center.Y;
		const double DZ = PZ - T.Center.Z;
		const double D2 = DX * DX + DY * DY + DZ * DZ;
		if (D2 <= T.Radius * T.Radius)
		{
			BestId = T.Id;
			bPeaceful = (T.Status == 0);
			break;
		}
	}
	CurrentTerritoryId = BestId;
	bInPeacefulZone = bPeaceful;
}

void UOUClientNet::SendBuild(int32 Entity, int32 Action, int32 X, int32 Y, int32 Z, int32 R, int32 G, int32 B)
{	if (!IsConnected())
	{
		return;
	}
	// Тело (тег 6): [tag][entity u32][action u8][x i32][y i32][z i32][r][g][b].
	TArray<uint8> Body;
	Body.Add(6);
	auto AddU32 = [&Body](uint32 V)
	{
		for (int32 i = 0; i < 4; ++i) { Body.Add(static_cast<uint8>((V >> (8 * i)) & 0xFF)); }
	};
	auto AddI32 = [&Body](int32 V)
	{
		const uint32 U = static_cast<uint32>(V);
		for (int32 i = 0; i < 4; ++i) { Body.Add(static_cast<uint8>((U >> (8 * i)) & 0xFF)); }
	};
	AddU32(static_cast<uint32>(Entity));
	Body.Add(static_cast<uint8>(Action & 0xFF));
	AddI32(X); AddI32(Y); AddI32(Z);
	Body.Add(static_cast<uint8>(FMath::Clamp(R, 0, 255)));
	Body.Add(static_cast<uint8>(FMath::Clamp(G, 0, 255)));
	Body.Add(static_cast<uint8>(FMath::Clamp(B, 0, 255)));
	SendPacket(Body);
}

void UOUClientNet::SendBuildAck(int32 Entity, int64 Revision)
{
	if (!IsConnected())
	{
		return;
	}
	// Тело (тег 10): [tag][entity varint][revision varint].
	TArray<uint8> Body;
	Body.Add(10);
	auto AddVarint = [&Body](uint64 V)
	{
		while (V >= 0x80)
		{
			Body.Add(static_cast<uint8>(V) | 0x80);
			V >>= 7;
		}
		Body.Add(static_cast<uint8>(V));
	};
	AddVarint(static_cast<uint64>(Entity));
	AddVarint(static_cast<uint64>(Revision));
	SendPacket(Body);
}

void UOUClientNet::Poll()
{
	if (!Socket)
	{
		return;
	}

	// Периодически шлём пинг — так HUD всегда показывает свежий реальный RTT
	// (и пешком, и в корабле: пеший не шлёт input с seq).
	if (IsConnected())
	{
		if (UWorld* W = GetWorld())
		{
			const double Now = W->GetTimeSeconds();
			if (Now >= NextPingTime)
			{
				NextPingTime = Now + 1.0;
				SendPing();
			}
		}
	}
	// Позы чужих персонажей собираются заново каждый кадр (кадровый буфер).
	RemoteCharacters.Reset();
	// Адрес отправителя создаём через фабрику: FInternetAddr абстрактный.
	ISocketSubsystem* SocketSub = ISocketSubsystem::Get(PLATFORM_SOCKETSUBSYSTEM);
	TSharedRef<FInternetAddr> From = SocketSub->CreateInternetAddr();

	// Читаем все доступные пакеты за кадр.
	TArray<uint8> Buffer;
	Buffer.SetNumUninitialized(1500);
	for (;;)
	{
		int32 BytesRead = 0;
		if (!Socket->RecvFrom(Buffer.GetData(), Buffer.Num(), BytesRead, *From) || BytesRead <= 0)
		{
			break;
		}
		// Проверяем токен: первые 8 байт.
		if (BytesRead < 12)
		{
			continue;
		}
		uint64 Token = 0;
		for (int32 i = 0; i < 8; ++i)
		{
			Token |= static_cast<uint64>(Buffer[i]) << (8 * i);
		}
		if (Token != static_cast<uint64>(SessionToken))
		{
			continue;
		}
		// Тело: с 12-го байта.
		TArray<uint8> Body;
		Body.Append(Buffer.GetData() + 12, BytesRead - 12);
		if (Body.Num() == 0)
		{
			continue;
		}
		if (Body[0] == OUWire::TagWelcome) // приветствие: наш id
		{
			int32 Entity = 0;
			if (OUWire::DecodeWelcome(Body.GetData(), static_cast<size_t>(Body.Num()), Entity))
			{
				OwnEntityId = Entity;
				State = EOUConnState::Connected;
				UE_LOG(LogOUClient, Log, TEXT("Сервер принял: моя сущность = %d"), Entity);
			}
		}
		else if (Body[0] == OUWire::TagSnapshotV2) // snapshot v2
		{
			FOUSnapshot Snap;
			if (ParseSnapshot(Body, Snap))
			{
				LatestSnapshot = Snap;
				++SnapshotsReceived;
				State = EOUConnState::Connected;
			}
		}
		else if (Body[0] == OUWire::TagBuildEdit) // авторитетная правка конструкции
		{
			OUWire::BuildEdit E;
			if (OUWire::DecodeBuildEdit(Body.GetData(), static_cast<size_t>(Body.Num()), E))
			{
				FOUBuildEdit Edit;
				Edit.Entity = static_cast<int32>(E.Entity);
				Edit.Author = static_cast<int32>(E.Author);
				Edit.Action = static_cast<int32>(E.Action);
				Edit.X = E.X; Edit.Y = E.Y; Edit.Z = E.Z;
				Edit.R = E.R; Edit.G = E.G; Edit.B = E.B;
				Edit.Revision = static_cast<int64>(E.Revision);
				PendingBuildEdits.Add(Edit);
			}
		}
		else if (Body[0] == OUWire::TagBuildAccess) // права на конструкцию
		{
			OUWire::BuildAccess A;
			if (OUWire::DecodeBuildAccess(Body.GetData(), static_cast<size_t>(Body.Num()), A))
			{
				bCanBuild = A.CanEdit;
			}
		}
		else if (Body[0] == OUWire::TagBuildResult) // результат моей команды
		{
			OUWire::BuildResult R;
			if (OUWire::DecodeBuildResult(Body.GetData(), static_cast<size_t>(Body.Num()), R))
			{
				bLastBuildAccepted = R.Accepted;
				// Пишем причину: без этого отказ сервера выглядит как «ничего
				// не произошло» и непонятно, что чинить.
				static const TCHAR* ReasonText[] = {
					TEXT("ok"), TEXT("недопустимо"), TEXT("нет связи с ядром"), TEXT("лимит")
				};
				const int32 Ri = FMath::Clamp(static_cast<int32>(R.Reason), 0, 3);
				UE_LOG(LogTemp, Log, TEXT("Строительство: сервер ответил '%s'"), ReasonText[Ri]);
			}
		}
		else if (Body[0] == OUWire::TagShot) // событие выстрела
		{
			OUWire::ShotEvent E;
			if (OUWire::DecodeShot(Body.GetData(), static_cast<size_t>(Body.Num()), E))
			{
				FOUShotEvent S;
				S.Attacker = static_cast<int32>(E.Attacker);
				S.Target = static_cast<int32>(E.Target);
				S.bHit = E.Hit;
				S.Damage = static_cast<int32>(E.Damage);
				S.bDestroyed = E.Destroyed;
				PendingShots.Add(S);
			}
		}
		else if (Body[0] == OUWire::TagHull) // состояние корпуса
		{
			OUWire::HullState H;
			if (OUWire::DecodeHull(Body.GetData(), static_cast<size_t>(Body.Num()), H))
			{
				if (static_cast<int32>(H.Entity) == OwnEntityId)
				{
					OwnHull.Entity = static_cast<int32>(H.Entity);
					OwnHull.Shield = static_cast<int32>(H.Shield);
					OwnHull.Armor = static_cast<int32>(H.Armor);
					OwnHull.bDestroyed = H.Destroyed;
				}
			}
		}
		else if (Body[0] == OUWire::TagDeath) // смерть
		{
			OUWire::DeathEvent D;
			if (OUWire::DecodeDeath(Body.GetData(), static_cast<size_t>(Body.Num()), D))
			{
				if (static_cast<int32>(D.Entity) == OwnEntityId)
				{
					bOwnDestroyed = true;
					OwnRespawnTick = static_cast<int64>(D.RespawnTick);
				}
			}
		}
		else if (Body[0] == OUWire::TagRespawn) // возрождение
		{
			OUWire::RespawnEvent R;
			if (OUWire::DecodeRespawn(Body.GetData(), static_cast<size_t>(Body.Num()), R))
			{
				if (static_cast<int32>(R.Entity) == OwnEntityId)
				{
					bOwnDestroyed = false;
					OwnRespawnTick = 0;
					OwnInvulnUntil = static_cast<int64>(R.InvulnUntil);
				}
			}
		}
		else if (Body[0] == OUWire::TagServerCharacter) // поза чужого персонажа
		{
			OUWire::CharacterPose P;
			if (OUWire::DecodeCharacter(Body.GetData(), static_cast<size_t>(Body.Num()), P))
			{
				if (static_cast<int32>(P.Entity) != OwnEntityId)
				{
					FOUCharacterState S;
					S.Entity = static_cast<int32>(P.Entity);
					// Провод → см UE → серверные единицы (как у сущностей).
					const double ToWorld = 1.0 / OUInterop::ServerUnitToCm;
					S.Position = FVector(P.X, P.Y, P.Z) * ToWorld;
					S.Yaw = P.Yaw;
					S.bMoving = P.Moving;
					RemoteCharacters.Add(S);
				}
			}
		}
		else if (Body[0] == OUWire::TagSystem) // описание системы
		{
			OUWire::SystemInfo Info;
			if (OUWire::DecodeSystem(Body.GetData(), static_cast<size_t>(Body.Num()), Info))
			{
				SystemName = FString(UTF8_TO_TCHAR(Info.Name.c_str()));
				SystemBodies.Reset(static_cast<int32>(Info.Bodies.size()));
				for (const OUWire::SystemBody& B : Info.Bodies)
				{
					FOUSystemBody S;
					S.Id = static_cast<int32>(B.Id);
					S.Kind = static_cast<int32>(B.Kind);
					S.Surface = static_cast<int32>(B.Surface);
					S.Center = FVector(B.CenterX, B.CenterY, B.CenterZ);
					S.Orbit = B.Orbit;
					S.Phase = B.Phase;
					S.Omega = B.Omega;
					S.Radius = static_cast<float>(B.Radius);
					S.Amplitude = B.Amplitude;
					S.Seed = static_cast<int64>(B.Seed);
					SystemBodies.Add(S);				}
				bSystemReceived = true;
			}
		}
		else if (Body[0] == OUWire::TagDig) // воронка от сервера
		{
			OUWire::DentWire D;
			if (OUWire::DecodeDig(Body.GetData(), static_cast<size_t>(Body.Num()), D))
			{
				FOUTerrainDent Dent;
				Dent.Slot = static_cast<int32>(D.Slot);
				Dent.X = D.X; Dent.Y = D.Y; Dent.Z = D.Z;
				Dent.CosRadius = D.CosRadius;
				Dent.Depth = D.Depth;
				// Воронки копятся: планета помнит все ямы. Слот уникален, поэтому
				// повторный приход того же слота — это углубление той же ямы.
				PendingDents.Add(Dent);
			}
		}
		else if (Body[0] == OUWire::TagDigYield) // что добыто при копании
		{
			OUWire::DigYield Y;
			if (OUWire::DecodeDigYield(Body.GetData(), static_cast<size_t>(Body.Num()), Y))
			{
				LastDigItem = static_cast<int32>(Y.Item);
				LastDigQty = static_cast<int64>(Y.Qty);
				LastDigLayer = FMath::Clamp(static_cast<int32>(Y.Layer), 0, 3);
				LastDigHoleDepth = static_cast<float>(Y.HoleDepth);
				DigTotalByLayer[LastDigLayer] += LastDigQty;
				if (UWorld* W = GetWorld())
				{
					LastDigTime = W->GetTimeSeconds();
				}
				UE_LOG(LogOUClient, Log, TEXT("Добыто: предмет %d ×%lld со слоя %d (глубина %.0f м)"),
					LastDigItem, LastDigQty, LastDigLayer, Y.HoleDepth);
			}
		}
		else if (Body[0] == OUWire::TagPong) // ответ на пинг — считаем RTT
		{
			uint32_t Nonce = 0;
			if (OUWire::DecodePong(Body.GetData(), static_cast<size_t>(Body.Num()), Nonce))
			{
				if (const double* SentAt = PingSentTime.Find(static_cast<int32>(Nonce)))
				{
					if (UWorld* W = GetWorld())
					{
						const float Rtt = static_cast<float>((W->GetTimeSeconds() - *SentAt) * 1000.0);
						if (Rtt >= 0.f && Rtt < 5000.f)
						{
							PingMs = Rtt;
							SmoothedPingMs = (SmoothedPingMs < 0.f) ? Rtt : FMath::Lerp(SmoothedPingMs, Rtt, 0.3f);
						}
					}
					PingSentTime.Remove(static_cast<int32>(Nonce));
				}
			}
		}
		else if (Body[0] == OUWire::TagRecipes) // каталог рецептов производства
		{
			std::vector<OUWire::Recipe> Rp;
			if (OUWire::DecodeRecipes(Body.GetData(), static_cast<size_t>(Body.Num()), Rp))
			{
				Recipes.Reset(static_cast<int32>(Rp.size()));
				for (const OUWire::Recipe& R : Rp)
				{
					FOURecipe E;
					E.Id = static_cast<int32>(R.Id);
					E.Name = FString(UTF8_TO_TCHAR(R.Name.c_str()));
					for (const auto& In : R.Inputs)
					{
						FOURecipeItem It;
						It.Item = static_cast<int32>(In.first);
						It.Qty = static_cast<int64>(In.second);
						E.Inputs.Add(It);
					}
					E.OutItem = static_cast<int32>(R.OutItem);
					E.OutQty = static_cast<int64>(R.OutQty);
					Recipes.Add(E);
				}
				bRecipesReceived = true;
				UE_LOG(LogOUClient, Log, TEXT("Каталог рецептов: %d"), Recipes.Num());
			}
		}
		else if (Body[0] == OUWire::TagCraftResult) // результат крафта
		{
			OUWire::CraftResult C;
			if (OUWire::DecodeCraftResult(Body.GetData(), static_cast<size_t>(Body.Num()), C))
			{
				bLastCraftOk = C.Ok;
				LastCraftItem = static_cast<int32>(C.Item);
				LastCraftQty = static_cast<int64>(C.Qty);
				LastCraftReason = FString(UTF8_TO_TCHAR(C.Reason.c_str()));
				if (UWorld* W = GetWorld())
				{
					LastCraftTime = W->GetTimeSeconds();
				}
				UE_LOG(LogOUClient, Log, TEXT("Крафт: %s, предмет %d ×%lld %s"),
					C.Ok ? TEXT("ок") : TEXT("отказ"), LastCraftItem, LastCraftQty,
					C.Ok ? TEXT("") : *LastCraftReason);
			}
		}
		else if (Body[0] == OUWire::TagTerritory) // территории (владение, налог)
		{
			std::vector<OUWire::Territory> Ts;
			if (OUWire::DecodeTerritory(Body.GetData(), static_cast<size_t>(Body.Num()), Ts))
			{
				Territories.Reset();
				for (const OUWire::Territory& T : Ts)
				{
					FOUTerritory E;
					E.Id = static_cast<int32>(T.Id);
					E.Owner = static_cast<int32>(T.Owner);
					E.Status = static_cast<int32>(T.Status);
					// Серверные единицы — как в системе (миникарта считает сама).
					E.Center = FVector(T.X, T.Y, T.Z);
					E.Radius = T.Radius;
					E.ControlShield = static_cast<int32>(T.ControlShield);
					E.TaxPercent = static_cast<int32>(T.TaxPercent);
					E.CollectedTax = static_cast<int64>(T.CollectedTax);
					Territories.Add(E);
				}
				bTerritoriesReceived = true;
				if (UWorld* W = GetWorld())
				{
					LastTerritoryTime = W->GetTimeSeconds();
				}
				UE_LOG(LogOUClient, Log, TEXT("Территории: %d зон"), Territories.Num());
			}
		}
		else if (Body[0] == OUWire::TagScripts) // библиотека скриптов
		{
			std::vector<OUWire::ScriptTemplateInfo> Ts;
			if (OUWire::DecodeScripts(Body.GetData(), static_cast<size_t>(Body.Num()), Ts))
			{
				ScriptTemplates.Reset();
				for (const OUWire::ScriptTemplateInfo& T : Ts)
				{
					FOUScriptTemplate E;
					E.Id = static_cast<int32>(T.Id);
					E.Name = FString(UTF8_TO_TCHAR(T.Name.c_str()));
					E.Description = FString(UTF8_TO_TCHAR(T.Description.c_str()));
					ScriptTemplates.Add(E);
				}
				bScriptsReceived = true;
				UE_LOG(LogOUClient, Log, TEXT("Скрипты: %d шаблонов"), ScriptTemplates.Num());
			}
		}
		else if (Body[0] == OUWire::TagScript) // статус скрипта на объекте
		{
			OUWire::ScriptStatus S;
			if (OUWire::DecodeScriptStatus(Body.GetData(), static_cast<size_t>(Body.Num()), S))
			{
				ScriptTemplate = static_cast<int32>(S.Template);
				ScriptName = FString(UTF8_TO_TCHAR(S.Name.c_str()));
				ScriptRuns = static_cast<int64>(S.Runs);
				ScriptErrors = static_cast<int64>(S.Errors);
				ScriptLastError = FString(UTF8_TO_TCHAR(S.Last.c_str()));
				ScriptLogs.Reset();
				for (const std::string& L : S.Logs)
				{
					ScriptLogs.Add(FString(UTF8_TO_TCHAR(L.c_str())));
				}
				bScriptStatusReceived = true;
			}
		}
		else if (Body[0] == OUWire::TagBlueprints) // библиотека шаблонов постройки
		{
			std::vector<OUWire::BlueprintInfo> Bs;
			if (OUWire::DecodeBlueprints(Body.GetData(), static_cast<size_t>(Body.Num()), Bs))
			{
				Blueprints.Reset();
				for (const OUWire::BlueprintInfo& B : Bs)
				{
					FOUBlueprint E;
					E.Id = static_cast<int32>(B.Id);
					E.Name = FString(UTF8_TO_TCHAR(B.Name.c_str()));
					E.Description = FString(UTF8_TO_TCHAR(B.Description.c_str()));
					Blueprints.Add(E);
				}
				bBlueprintsReceived = true;
				UE_LOG(LogOUClient, Log, TEXT("Шаблоны постройки: %d"), Blueprints.Num());
			}
		}
		else if (Body[0] == OUWire::TagMarketState) // состояние экономики
		{
			OUWire::MarketState M;
			if (OUWire::DecodeMarketState(Body.GetData(), static_cast<size_t>(Body.Num()), M))
			{
				Credits = static_cast<int64>(M.Credits);
				InventoryItems.Reset();
				for (const auto& it : M.Items)
				{
					FOUInventoryItem E;
					E.Item = static_cast<int32>(it.first);
					E.Qty = static_cast<int64>(it.second);
					InventoryItems.Add(E);
				}
				MarketBids.Reset();
				for (const auto& b : M.Bids)
				{
					FOUOrderLevel L;
					L.Item = static_cast<int32>(std::get<0>(b));
					L.Price = static_cast<int64>(std::get<1>(b));
					L.Qty = static_cast<int64>(std::get<2>(b));
					MarketBids.Add(L);
				}
				MarketAsks.Reset();
				for (const auto& a : M.Asks)
				{
					FOUOrderLevel L;
					L.Item = static_cast<int32>(std::get<0>(a));
					L.Price = static_cast<int64>(std::get<1>(a));
					L.Qty = static_cast<int64>(std::get<2>(a));
					MarketAsks.Add(L);
				}
				bMarketReceived = true;
			}
		}
	}
}

bool UOUClientNet::ParseSnapshot(const TArray<uint8>& Body, FOUSnapshot& Out)
{
	// Разбор делегирован OUWire — тому же коду, который проверен на реальном
	// пакете сервера стендом `client/testdata/test_wire.cpp`. Дублирования нет,
	// а значит клиент и тест не могут разойтись.
	OUWire::Snapshot Snap;
	if (!OUWire::DecodeSnapshot(Body.GetData(), static_cast<size_t>(Body.Num()), Snap))
	{
		return false;
	}

	Out.Tick = Snap.Tick;
	Out.AckInputSeq = Snap.AckInputSeq;
	Out.bBaseline = Snap.Baseline;
	LastServerTick = static_cast<int64>(Snap.Tick);

	// --- Реальный пинг: RTT по подтверждённому кадру ввода ---
	if (Snap.AckInputSeq > 0)
	{
		if (const double* SentAt = InputSentTime.Find(static_cast<int32>(Snap.AckInputSeq)))
		{
			if (UWorld* W = GetWorld())
			{
				const float Rtt = static_cast<float>((W->GetTimeSeconds() - *SentAt) * 1000.0);
				if (Rtt >= 0.f && Rtt < 5000.f)
				{
					PingMs = Rtt;
					SmoothedPingMs = (SmoothedPingMs < 0.f) ? Rtt : FMath::Lerp(SmoothedPingMs, Rtt, 0.25f);
				}
			}
			InputSentTime.Remove(static_cast<int32>(Snap.AckInputSeq));
		}
	}
	Out.Entities.Reset(static_cast<int32>(Snap.Entities.size()));
	for (const OUWire::EntityState& E : Snap.Entities)
	{
		FOUEntityState S;
		S.Id = E.Id;
		// OUWire уже перевёл провод в сантиметры UE (WireToCm), но оси остались
		// серверными. State хранит позицию в СЕРВЕРНЫХ ЕДИНИЦАХ (1 ед. = 100 см),
		// как и вся физика предсказания, — поэтому делим на масштаб. Иначе
		// ApplyStateToActor умножит ещё раз на 100 (двойной масштаб).
		const double ToWorld = 1.0 / OUInterop::ServerUnitToCm;
		S.Position = FVector(E.X, E.Y, E.Z) * ToWorld;
		S.Velocity = FVector(E.VX, E.VY, E.VZ) * ToWorld;
		S.Yaw = E.Yaw;
		S.Pitch = E.Pitch;
		Out.Entities.Add(S);
	}
	return true;
}
