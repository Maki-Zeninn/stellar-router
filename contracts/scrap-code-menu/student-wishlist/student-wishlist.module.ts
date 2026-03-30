import { Module } from '@nestjs/common';
import { StudentWishlistController } from './student-wishlist.controller';
import { StudentWishlistService } from './student-wishlist.service';

@Module({
  controllers: [StudentWishlistController],
  providers: [StudentWishlistService],
})
export class StudentWishlistModule {}
