// Copyright Open Universe. Все права защищены.

#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Actor.h"
#include "OpenUniverseTypes.h"
#include "OUTerrain.h"
#include "OUVoxelPlanet.generated.h"

class UProceduralMeshComponent;
class UStaticMeshComponent;

/** Уровень детализации (совпадает с серверным LOD). */
UENUM(BlueprintType)
enum class EOUVoxelLod : uint8
{
	Far UMETA(DisplayName = "Далеко (4×4)"),
	Mid UMETA(DisplayName = "Средне (8×8)"),
	Near UMETA(DisplayName = "Близко (16×16)"),
	Close UMETA(DisplayName = "Вплотную (24×24)")
};

/**
 * Тип планеты — задаёт палитру поверхности и характер рельефа.
 * Нужен, чтобы планеты в системе были визуально разными.
 */
UENUM(BlueprintType)
enum class EOUPlanetType : uint8
{
	/** Землеподобная: океан, трава, снег, плотная атмосфера и облака. */
	Terran UMETA(DisplayName = "Землеподобная"),
	/** Пустынная: пески, дюны, каньоны; тонкая пыльная атмосфера, без облаков. */
	Desert UMETA(DisplayName = "Пустынная"),
	/** Ледяная: снег и голубой лёд; атмосфера и облака. */
	Ice UMETA(DisplayName = "Ледяная"),
	/** Вулканическая: тёмная порода и раскалённые трещины; плотная дымка. */
	Volcanic UMETA(DisplayName = "Вулканическая"),
	/** Газовая: без рельефа, полосы (юпитер-подобная); плотная атмосфера. */
	GasGiant UMETA(DisplayName = "Газовый гигант"),
	/** Безвоздушная (луна): голый камень и кратеры, НЕТ атмосферы и облаков. */
	Airless UMETA(DisplayName = "Безвоздушная")
};

/**
 * Воксельная планета.
 *
 * Повторяет серверную геометрию (`crates/render/src/voxel.rs`):
 * кубическая сетка искривляется в сферу правильной формулой (cube-to-sphere),
 * высота берётся из шума. Это ровно тот же рельеф, что считает физика, —
 * поэтому посадка совпадает с картинкой.
 *
 * Меш строится процедурно (`UProceduralMeshComponent`) — по одной грани на
 * воксель. Для больших планет это заготовка: в реальном проекте нужен LOD и
 * стриминг чанков, что отражено в плане.
 */
UCLASS()
class OPENUNIVERSE_API AOUVoxelPlanet : public AActor
{
	GENERATED_BODY()

public:
	AOUVoxelPlanet();

	/** Радиус планеты (уровень океана), см UE. 500 км — как на сервере (500 000 ед. × 100). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Planet")
	float Radius = 50000000.f;

	/** Размах рельефа, см (10 км — крупные горы и долины в масштабе планеты). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Planet")
	float Amplitude = 1000000.f;

	/** Семя генерации (совпадает с сервером). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Planet")
	int64 Seed = 0xC0FFEE;

	/** Уровень детализации. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Planet")
	EOUVoxelLod Lod = EOUVoxelLod::Close;

	/** Тип планеты (палитра и характер рельефа). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Planet")
	EOUPlanetType PlanetType = EOUPlanetType::Terran;

	/** Можно ли строить/приземляться на этой планете (домашняя — да). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Planet")
	bool bHomePlanet = false;

	/** Построить меш планеты. */
	UFUNCTION(BlueprintCallable, CallInEditor, Category = "Open Universe|Planet")
	void BuildPlanet();

	/** Число сегментов на грань для выбранного LOD. */
	UFUNCTION(BlueprintPure, Category = "Open Universe|Planet")
	int32 Segments() const;

	/** Высота рельефа в направлении (для посадки и проверок). */
	UFUNCTION(BlueprintPure, Category = "Open Universe|Planet")
	float HeightAt(const FVector& Direction) const;

	/** Высота в СЕРВЕРНЫХ осях (совпадает с сервером бит-в-бит). */
	float HeightAtServer(const FVector& DirServer) const;

	/**
	 * Высота в СЕРВЕРНЫХ осях с уровнем детализации.
	 *
	 * `Detail = 0` — крупный рельеф (глобальный меш планеты: ячейка ~1.5 км),
	 * `Detail = 1` — полный (физика сервера и локальный участок у игрока).
	 * См. `OUTerrain.h::HeightDir`.
	 */
	float HeightAtServerDetail(const FVector& DirServer, int32 Detail) const;

	/** Высота в UE-осях с уровнем детализации. */
	float HeightAtDetail(const FVector& Direction, int32 Detail) const;

	/** Радиус поверхности в направлении (UE-оси). */
	UFUNCTION(BlueprintPure, Category = "Open Universe|Planet")
	float SurfaceRadiusAt(const FVector& Direction) const;

	/** Радиус поверхности в СЕРВЕРНЫХ осях. */
	float SurfaceRadiusAtServer(const FVector& DirServer) const
	{
		return Radius + HeightAtServer(DirServer);
	}

	/** Направление точки появления в UE-осях (для камеры и ориентации). */
	UFUNCTION(BlueprintPure, Category = "Open Universe|Planet")
	static FVector HomeSpawnDirection();

	/** Направление точки появления в СЕРВЕРНЫХ осях (как сервер). */
	static FVector HomeSpawnDirectionServer();

	/**
	 * Насколько неровная земля вокруг направления в радиусе `RadiusM` (метры):
	 * перепад высот по кольцу проб. Зеркало серверной `spawn_roughness`.
	 */
	double SpawnRoughnessServer(const FVector& DirServer, double RadiusM) const;

	/**
	 * Рельеф площадки базы в направлении (см): плоский пол внутри и
	 * ВОКСЕЛЬНЫЕ ТЕРРАСЫ (ступени) по краю, а не гладкий пандус.
	 *
	 * Единая точка правды: визуальный меш планеты и земля персонажа берут эту
	 * функцию, поэтому пол базы ровный, а на базу поднимаешься по воксельным
	 * ступеням — как в Dual Universe.
	 */
	static double PadFlattenCm(const FVector& DirUE);

	/** Полурадиус ровной площадки базы, см (совпадает с базой). */
	static constexpr float PadFlatRadius = 9000.f;

	/** Насколько площадка приподнята над максимумом земли под ней, см. */
	static constexpr float PadLift = 40.f;

	/** Высота одной воксельной ступени площадки, см (1 м — как воксель DU). */
	static constexpr float PadVoxelCm = 100.f;

	/** Вылет одной воксельной ступени по горизонтали, см. */
	static constexpr float PadVoxelRunCm = 300.f;

	/** Внешний радиус влияния площадки (пол + террасы), см. */
	static constexpr float PadOuterRadiusCm = PadFlatRadius * 1.35f;

	/** Дуговое расстояние (см) от центра базы до направления (0 — центр). */
	static double PadDistanceCm(const FVector& DirUE);

	/**
	 * Абсолютная высота земли площадки (см над центром планеты) в направлении:
	 * рельеф + воксельные террасы `PadFlattenCm`. По ней строится воксельный меш
	 * площадки и ставятся постройки.
	 */
	static double PadGroundAboveCenterCm(const FVector& DirUE)
	{
		return AnalyticHeightCm(DirUE) + PadFlattenCm(DirUE);
	}

	/**
	 * Построить ВОКСЕЛЬНЫЙ меш площадки базы (ступени) вокруг её центра.
	 *
	 * Глобальный меш планеты (~1.5 км/ячейка) и плотный локальный участок
	 * (~21 м) слишком грубы, чтобы показать ступени по ~1 м. Поэтому площадка
	 * рисуется отдельным мелким мешем: верхние грани — по высоте террас, а на
	 * перепадах добавляются ВЕРТИКАЛЬНЫЕ стенки-воксели (иначе ступени были бы
	 * наклонными).
	 */
	void BuildPadMesh();

	/** Уровень пола площадки над центром планеты, см (макс. земля + подъём). */
	static double DeckLevelAboveCenterCm();

	/** Аналитическая высота рельефа планеты (см) в UE-направлении. */
	static double AnalyticHeightCm(const FVector& DirUE);

	/**
	 * Добавить/обновить воронку, выкопанную игроком (от сервера).
	 *
	 * Воронки хранятся списком слотов; повторный слот углубляет ту же яму.
	 * После добавления нужно перестроить меш (`RebuildLocal` + `BuildPlanet`).
	 */
	void ApplyDent(int32 Slot, double X, double Y, double Z, double CosRadius, double Depth);

	/** Сколько воронок сейчас учтено (для диагностики). */
	int32 DentCount() const { return Dents.Num(); }

	/** Воронки для общей формулы высоты (в серверных осях). */
	const TArray<OULT::Dent>& DentList() const { return Dents; }

	/** Точка появления в СЕРВЕРНЫХ единицах (как хранит корабль). */
	static FVector HomeSpawnServer(float Clearance);

	/**
	 * Найти домашнюю планету (ту, где стоит игрок и база).
	 *
	 * В системе несколько планет (`AOUVoxelPlanet`), поэтому обычный
	 * `GetActorOfClass` вернул бы любую. Домашняя помечена `bHomePlanet`.
	 */
	static AOUVoxelPlanet* FindHomePlanet(UWorld* World);

	/** Точка появления над поверхностью (на `Clearance` ед. сервера), UE-оси. */
	UFUNCTION(BlueprintPure, Category = "Open Universe|Planet")
	static FVector HomeSpawn(float Clearance);

	// --- Атмосфера и облака ---

	/** Есть ли у этого типа планеты атмосфера (газовый слой и свечение). */
	UFUNCTION(BlueprintPure, Category = "Open Universe|Planet")
	static bool TypeHasAtmosphere(EOUPlanetType Type);

	/** Есть ли у этого типа планеты облака. */
	UFUNCTION(BlueprintPure, Category = "Open Universe|Planet")
	static bool TypeHasClouds(EOUPlanetType Type);

	/** Цвет атмосферы (даёт ореол вида планеты). */
	UFUNCTION(BlueprintPure, Category = "Open Universe|Planet")
	static FLinearColor AtmosphereTint(EOUPlanetType Type);

	/** Собрать/обновить атмосферу и облака по типу планеты. */
	UFUNCTION(BlueprintCallable, CallInEditor, Category = "Open Universe|Planet")
	void BuildAtmosphere();

	/**
	 * Перестроить локальный детальный участок поверхности вокруг точки.
	 *
	 * Зачем: у всей планеты ячейка меша — километры, поэтому мелкий рельеф
	 * (холмы в сотни метров) у ног игрока не виден. Локальный участок — плотная
	 * сетка в касательной плоскости вокруг игрока, высоты берутся из ТОЙ ЖЕ
	 * формулы, что и глобальный меш/сервер.
	 *
	 * @param AroundUE  точка на поверхности (мировые см), вокруг которой строим
	 */
	void RebuildLocal(const FVector& AroundUE);

	/** Размер локального участка (полусторона, см) и число ячеек на сторону. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Planet")
	float LocalExtentCm = 400000.f;   // 4 км в каждую сторону (за горизонт)

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Planet")
	int32 LocalCells = 384;           // 768×768 вершин, ячейка ~21 м

	/** Максимум облачных «клубов» на планету (пул создаётся в конструкторе). */
	static constexpr int32 MaxCloudPuffs = 160;

	/** Воронки, выкопанные игроками (общий список, применяется в высоте). */
	TArray<OULT::Dent> Dents;

	/** Последний центр локального участка (задаёт персонаж) — для перестройки. */
	FVector LocalDentCenter = FVector(FLT_MAX, FLT_MAX, FLT_MAX);

protected:
	virtual void BeginPlay() override;

	/** Строим меш и в редакторе, чтобы планета была видна сразу на карте. */
	virtual void OnConstruction(const FTransform& Transform) override;

	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Planet")
	UProceduralMeshComponent* Mesh = nullptr;

	/** Локальный детальный участок поверхности вокруг игрока (отдельный меш). */
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Planet")
	UProceduralMeshComponent* LocalMesh = nullptr;

	/**
	 * Воксельный меш площадки базы (ступени). Отдельный мелкий меш: глобальный
	 * меш (~1.5 км/ячейка) и локальный участок (~21 м) не показывают ступени по
	 * ~1 м, поэтому площадка рисуется своей сеткой.
	 */
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Planet")
	UProceduralMeshComponent* PadMesh = nullptr;

	/** Слой атмосферы: чуть больше планеты, полупрозрачный ореол. */
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Planet")
	UStaticMeshComponent* AtmosphereMesh = nullptr;

	/** Облака: пул сфер над поверхностью (показываются по типу планеты). */
	UPROPERTY(VisibleAnywhere, BlueprintReadOnly, Category = "Open Universe|Planet")
	TArray<UStaticMeshComponent*> CloudPuffs;

	/** Материал планеты (назначается на все секции меша). */
	UPROPERTY(Transient)
	TObjectPtr<class UMaterialInterface> PlanetMaterial = nullptr;

	/** Цвет поверхности по высоте и типу планеты. */
	FLinearColor SurfaceColor(float Height) const;

	/**
	 * Цвет вершины с запечённым простым освещением.
	 *
	 * Материал планеты — отладочный `VertexColorMaterial` (не освещается движком):
	 * без этого рельеф выглядит однотонной плоскостью, потому что цвет вершины не
	 * зависит от наклона поверхности. Считаем «псевдо-свет»: солнце сверху-сбоку,
	 * диффуз по нормали (плюс немного) — так сглаженные склоны и хребты читаются.
	 */
	FLinearColor ShadedVertexColor(float Height, const FVector& SmoothNormal) const;

	/**
	 * Сглаженная нормаль поверхности в точке (конечными разностями по рельефу).
	 * Даёт рельефу «объём» на меше: плоские грани по-разному освещены.
	 */
	FVector SmoothSurfaceNormal(const FVector& DirUE, int32 Detail) const;

	/**
	 * Гашение мелкого рельефа у края локального участка.
	 *
	 * @param DistCm  расстояние от центра участка в касательной плоскости, см
	 * @return 1 в глубине участка, 0 на самой границе (и за ней)
	 *
	 * Нужно, чтобы край плотного участка совпал с глобальным мешем (там мелкого
	 * рельефа нет): иначе виден обрыв высоты в несколько сотен метров на линии
	 * горизонта. Внутри участка гашения нет — под игроком высота = физике.
	 */
	float LocalDetailFade(float DistCm) const;

	/**
	 * Высота рельефа для ВИЗУАЛЬНОГО меша с учётом характера типа планеты.
	 * Для домашней планеты совпадает с физической высотой (иначе игрок
	 * «висел» бы над землёй); для декоративных — добавляет дюны/кратеры/полосы.
	 */
	float VisualHeight(const FVector& DirUE) const;

	/**
	 * То же, но с уровнем детализации рельефа.
	 *
	 * `Detail = 0` — только крупный рельеф: так строится ГЛОБАЛЬНЫЙ меш планеты
	 * (ячейка ~1.5 км не может показать мелкие волны, иначе камера уходит под
	 * грань). `Detail = 1` — полный рельеф для плотного локального участка.
	 */
	float VisualHeightDetail(const FVector& DirUE, int32 Detail) const;

private:
	/** Детерминированный шум для облаков (независимый канал). */
	float CloudNoise(const FVector& Dir) const;

	/** Кэш базовых ассетов (сфера/материалы) — грузим лениво. */
	UPROPERTY(Transient)
	TObjectPtr<class UStaticMesh> CloudSphereMesh = nullptr;
	UPROPERTY(Transient)
	TObjectPtr<class UMaterialInterface> CloudMaterial = nullptr;
	UPROPERTY(Transient)
	TObjectPtr<class UMaterialInterface> AtmosphereMaterial = nullptr;
};
