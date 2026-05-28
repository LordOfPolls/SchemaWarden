-- junk ai generated stored proc for testing
CREATE PROCEDURE [dbo].[usp_ProcessOrderBatch]
    @BatchSize      INT             = 100,
    @ProcessedBy    NVARCHAR(128)   = N'SYSTEM',
    @StartDate      DATETIME        = NULL,
    @EndDate        DATETIME        = NULL,
    @DryRun         BIT             = 0
AS
BEGIN
    SET NOCOUNT ON;
    SET XACT_ABORT ON;

    /* ================================================================
       usp_ProcessOrderBatch
       ----------------------------------------------------------------
       Processes pending orders in batches, updating status and
       writing audit records.  Supports dry-run mode for validation.

       Nested comment: /* see ticket PROJ-4471 for batch-size cap */
    ================================================================ */

    DECLARE @BatchID        UNIQUEIDENTIFIER = NEWID();
    DECLARE @ProcessedCount INT              = 0;
    DECLARE @Now            DATETIME         = GETUTCDATE();

    -- Default date range to the last 30 days if not supplied
    IF @StartDate IS NULL
        SET @StartDate = DATEADD(DAY, -30, @Now);
    IF @EndDate IS NULL
        SET @EndDate = @Now;

    -- Validate: 0 < BatchSize <= 10000
    IF @BatchSize <= 0 OR @BatchSize > 10000
    BEGIN
        RAISERROR(N'BatchSize must be between 1 and 10,000.', 16, 1);
        RETURN -1;
    END

    /* Snapshot the IDs we'll process this run.
       TOP + ORDER BY gives a consistent, deterministic batch.
       Zero-value orders are intentionally excluded /* they are voided */. */
    CREATE TABLE #PendingOrders (
        OrderID    INT           NOT NULL,
        CustomerID INT           NOT NULL,
        OrderTotal DECIMAL(18,2) NOT NULL,
        PRIMARY KEY (OrderID)
    );

    INSERT INTO #PendingOrders (OrderID, CustomerID, OrderTotal)
    SELECT TOP (@BatchSize)
           o.OrderID,
           o.CustomerID,
           o.OrderTotal
    FROM   [dbo].[Orders]    AS o  -- source table; never query the archive schema here
    JOIN   [dbo].[Customers] AS c  ON  c.CustomerID = o.CustomerID
                                   AND c.[Status]   = N'Active'   -- skip suspended accounts
    WHERE  o.[Status]          = N'Pending'
      AND  o.CreatedAt        >= @StartDate
      AND  o.CreatedAt        <  @EndDate
      AND  o.OrderTotal        > 0.00
    ORDER BY o.CreatedAt ASC, o.OrderID ASC;

    IF @@ROWCOUNT = 0
    BEGIN
        -- Nothing to process; write a log row and exit cleanly
        INSERT INTO [dbo].[BatchLog] (BatchID, StartedAt, FinishedAt, ProcessedCount, Notes)
        VALUES (@BatchID, @Now, GETUTCDATE(), 0, N'No eligible orders found.');
        RETURN 0;
    END

    BEGIN TRY
        BEGIN TRANSACTION;

        /* ---- Main status update ---------------------------------- */
        UPDATE o
        SET    o.[Status]     = CASE
                                    -- high-value orders skip the queue entirely
                                    WHEN po.OrderTotal >= 1000.00 THEN N'Approved'
                                    ELSE N'Processing'
                                END,
               o.ProcessedAt = @Now,
               o.ProcessedBy = @ProcessedBy,
               o.BatchID     = @BatchID
        FROM   [dbo].[Orders]  AS o
        JOIN   #PendingOrders  AS po ON po.OrderID = o.OrderID
        WHERE  @DryRun = 0;  -- omit write entirely in dry-run mode

        SET @ProcessedCount = @@ROWCOUNT;

        /* ---- Audit trail ----------------------------------------- */
        INSERT INTO [dbo].[OrderAudit] (
            OrderID, OldStatus,  NewStatus,
            ChangedAt, ChangedBy, BatchID
        )
        SELECT po.OrderID,
               N'Pending',
               CASE WHEN po.OrderTotal >= 1000.00 THEN N'Approved' ELSE N'Processing' END,
               @Now,
               @ProcessedBy,
               @BatchID
        FROM   #PendingOrders AS po
        WHERE  @DryRun = 0;

        INSERT INTO [dbo].[BatchLog] (BatchID, StartedAt, FinishedAt, ProcessedCount, Notes)
        VALUES (
            @BatchID,
            @Now,
            GETUTCDATE(),
            @ProcessedCount,
            N'Completed. DryRun=' + CAST(@DryRun AS NVARCHAR(1))
        );

        COMMIT TRANSACTION;
    END TRY
    BEGIN CATCH
        IF @@TRANCOUNT > 0
            ROLLBACK TRANSACTION;

        -- Capture details before re-raising so the log row is always written
        DECLARE @ErrMsg  NVARCHAR(4000) = ERROR_MESSAGE();
        DECLARE @ErrLine INT            = ERROR_LINE();

        INSERT INTO [dbo].[BatchLog] (BatchID, StartedAt, FinishedAt, ProcessedCount, Notes)
        VALUES (
            @BatchID, @Now, GETUTCDATE(), 0,
            N'ERROR line ' + CAST(@ErrLine AS NVARCHAR(10)) + N': ' + @ErrMsg
        );

        THROW;  -- re-raise; caller decides whether to retry
    END CATCH

    DROP TABLE IF EXISTS #PendingOrders;

    RETURN @ProcessedCount;
END
GO
