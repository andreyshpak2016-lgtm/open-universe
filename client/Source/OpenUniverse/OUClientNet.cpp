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
	// Кадр (тег 2): [tag][count u16][seq u32][throttle i16][yaw i16][pitch i16][fire u8]
	TArray<uint8> Body;
	Body.Add(2);
	Body.Add(1); // count = 1
	Body.Add(0);

	const uint32 Seq = static_cast<uint32>(Input.Seq);
	for (int32 i = 0; i < 4; ++i)
	{
		Body.Add(static_cast<uint8>((Seq >> (8 * i)) & 0xFF));
	}
	// Квантование −1..1 в i16 (масштаб 32767, как на сервере).
	auto Quantize = [](float V) -> int16
	{
		const float Clamped = FMath::Clamp(V, -1.f, 1.f);
		return static_cast<int16>(FMath::RoundToInt(Clamped * 32767.f));
	};
	auto AddI16 = [&Body](int16 V)
	{
		Body.Add(static_cast<uint8>(V & 0xFF));
		Body.Add(static_cast<uint8>((V >> 8) & 0xFF));
	};
	AddI16(Quantize(Input.Throttle));
	AddI16(Quantize(Input.Yaw));
	AddI16(Quantize(Input.Pitch));
	Body.Add(Input.bFire ? 1 : 0);

	SendPacket(Body);
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

void UOUClientNet::SendBuild(int32 Entity, int32 Action, int32 X, int32 Y, int32 Z, int32 R, int32 G, int32 B)
{
	if (!IsConnected())
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
					SystemBodies.Add(S);
				}
				bSystemReceived = true;
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
