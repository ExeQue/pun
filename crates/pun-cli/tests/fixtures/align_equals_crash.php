<?php
use Illuminate\Database\Eloquent\Relations\MorphTo;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\Attributes\Group;
use PHPUnit\Framework\Attributes\Test;
use Zebra\API\Common\Http\Middleware\SubstituteBindings;
use Zebra\API\V1\Http\Controllers\Project\ProjectTimesheetController;
use Zebra\API\V1\Http\Resources\Task\Timesheet;
use Zebra\Exceptions\InvalidArgumentsException;
use Zebra\Models\Framework\FrameworkTypeCourtCase;
use Zebra\Models\Framework\FrameworkTypeIntermApp;
use Zebra\Models\Project\Project;
use Zebra\Models\Project\ProjectTaskActualExpense;
use Zebra\Models\Tasks\ProjectTask;
use Zebra\Tests\API\V1\TestCaseAPI;
/**
 */
final class IndexTest extends TestCaseAPI
{
    /**
     */
    public function get_project_timesheet_data_sort_by_created_at_ascending(): void
    {
        [$oldest, $middle, $newest] = $this->createOutOfIdOrder(ProjectTask::factory(), 'created_at', ['project_id' => $project->id, 'type_id' => 1]);
        $response = $this->ajaxGet(\route('api.v1.cases.timesheets.index', [
            'case' => $this->encodeId($project->id),
            'sort' => 'created_at',
        ]))->assertSuccessful();
        $this->assertDataIdsInOrder($response, [$oldest->id, $middle->id, $newest->id]);
    }
    /**
     * @author George X
     */
    #[Group('controllers')]
    #[Group('api')]
    public function get_project_timesheet_data_sort_by_created_at_descending(): void
    {
        $response = $this->ajaxGet(\route('api.v1.cases.timesheets.index', [
        ]))->assertSuccessful();
    }
}
