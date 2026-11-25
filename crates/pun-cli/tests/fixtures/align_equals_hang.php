<?php
namespace Zebra\Tests\API\V1\Http\Controllers\Project\ProjectFrameworkWorkingController;
use Illuminate\Database\Eloquent\Relations\MorphTo;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\Attributes\Group;
use PHPUnit\Framework\Attributes\Test;
use Zebra\API\Common\Http\Middleware\SubstituteBindings;
use Zebra\API\V1\Http\Controllers\Project\ProjectFrameworkWorkingController;
use Zebra\API\V1\Http\Resources\Framework\FrameworkWorking as FrameworkWorkingResource;
use Zebra\Enums\DateFormat;
use Zebra\Exceptions\InvalidArgumentsException;
use Zebra\Models\Framework\FrameworkTypeCourtCase;
use Zebra\Models\Framework\FrameworkTypeIntermApp;
use Zebra\Models\Framework\FrameworkWorking;
use Zebra\Models\Project\Project;
use Zebra\Tests\API\V1\TestCaseAPI;
/**
 */
final class IndexTest extends TestCaseAPI
{
    /**
     */
    public function when_user_is_not_logged_in_auth_error(): void
    {
        $this->ajaxGet(\route('api.v1.cases.proceedings.index', ['case' => $project->id]))
             ->assertFailedUnauthorized();
    }
    /**
     */
    public function get_check_response_successfully(): void
    {
        $work = FrameworkWorking::with([
            'tfData' => function (MorphTo $morphTo): void {
                $morphTo->morphWith([
                    FrameworkTypeCourtCase::class   => ['scale', 'district', 'courtType'],
                    FrameworkTypeIntermApp::class   => ['scale', 'district', 'courtType'],
                ]);
            },
        ])->whereIn('id', $workings->pluck('id')->toArray())->paginate();
        $resourceData = FrameworkWorkingResource::collection($work)
                                                ->response()
                                                ->getData(true);
        $this->ajaxGet(\route('api.v1.cases.proceedings.index', ['case' => $this->encodeId($project->id)]))->assertOk()->assertJson([
            'data' => $resourceData['data'],
        ]);
    }
    /**
     * @author George X
     */
    #[Group('controllers')]
    #[Group('api')]
    public function get_project_framework_working_data_sort_by_created_at_descending(): void
    {
        $project          = Project::factory()->createQuietly();
        [$oldest, $middle, $newest] = $this->createOutOfIdOrder(FrameworkWorking::factory(), 'created_at', ['project_id' => $project->id]);
        $response = $this->ajaxGet(\route('api.v1.cases.proceedings.index', [
        ]))->assertSuccessful();
    }
}
