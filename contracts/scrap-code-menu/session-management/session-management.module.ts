import { Module } from '@nestjs/common';
import { SessionManagementController } from './session-management.controller';
import { SessionManagementService } from './session-management.service';

@Module({
  controllers: [SessionManagementController],
  providers: [SessionManagementService],
})
export class SessionManagementModule {}
